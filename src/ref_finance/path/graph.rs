use crate::logging::*;
use crate::ref_finance::errors::Error;
use crate::ref_finance::path::by_token::PoolsByToken;
use crate::ref_finance::path::edge::EdgeWeight;
use crate::ref_finance::pool_info::{PoolInfoList, TokenPair};
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use crate::Result;
use petgraph::algo;
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::ops::Add;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct TokenGraph {
    pools: PoolInfoList,
    graph: CachedPath<TokenInAccount, TokenOutAccount, TokenAccount, EdgeWeight>,
}

impl TokenGraph {
    pub fn new(pools: PoolInfoList) -> Self {
        let pools_by_token = PoolsByToken::new(pools.clone());
        let mut graph = petgraph::Graph::new();
        let mut nodes = HashMap::new();
        for token_in in pools_by_token.tokens() {
            let node = graph.add_node(token_in.clone());
            nodes.insert(token_in, node);
        }
        for (token_in, &node_in) in nodes.iter() {
            let edges_by_token_out = pools_by_token.get_groups_by_out(&token_in.clone().into());
            for (token_out, edges) in edges_by_token_out.iter() {
                for edge in edges.at_top().into_iter() {
                    for &node_out in nodes.get(&token_out.clone().into()).into_iter() {
                        graph.add_edge(node_in, node_out, edge.weight());
                    }
                }
            }
        }
        Self {
            pools,
            graph: CachedPath::new(graph, nodes, Error::TokenNotFound, Error::NoValidEddge),
        }
    }

    pub fn list_returns(
        &self,
        initial: u128,
        start: TokenInAccount,
    ) -> Result<Vec<(TokenOutAccount, u128)>> {
        let goals = self.graph.update_path(start.clone(), None)?;
        for goal in goals.iter() {
            self.graph.update_path(goal.as_in(), Some(start.as_out()))?;
        }

        let mut returns = HashMap::new();
        for goal in goals.into_iter() {
            let value = self.estimate_return(initial, start.clone(), goal.clone())?;
            returns.insert(goal, value);
        }
        let mut returns: Vec<_> = returns.into_iter().collect();
        returns.sort_by_key(|(_, value)| *value);
        returns.reverse();
        Ok(returns)
    }

    fn estimate_return(
        &self,
        initial: u128,
        start: TokenInAccount,
        goal: TokenOutAccount,
    ) -> Result<u128> {
        if initial == 0 {
            return Ok(0);
        }
        let mut value = initial;

        let pairs = self.get_path_with_return(start.clone(), goal.clone())?;
        for pair in pairs.iter() {
            value = pair.estimate_return(value)?;
            if value == 0 {
                return Ok(0);
            }
        }

        Ok(value)
    }

    fn get_path(&self, start: TokenInAccount, goal: TokenOutAccount) -> Result<Vec<TokenPair>> {
        let mut result = Vec::new();
        let edges = self.graph.get_edges(start.clone(), goal.clone())?;
        for edge in edges.iter() {
            let pair_id = edge.pair_id.expect("should be pair id");
            let pair = self.pools.get_pair(pair_id)?;
            result.push(pair);
        }
        Ok(result)
    }

    // 往路と復路のパスを TokenPair のリストで返す
    pub fn get_path_with_return(
        &self,
        start: TokenInAccount,
        goal: TokenOutAccount,
    ) -> Result<Vec<TokenPair>> {
        let mut path = self.get_path(start.clone(), goal.clone())?;
        path.extend(self.get_path(goal.as_in(), start.as_out())?);
        Ok(path)
    }
}

type PathToOut<O, N> = HashMap<O, Vec<N>>;

#[derive(Debug)]
struct CachedPath<I, O, N, E> {
    graph: petgraph::Graph<N, E>,
    nodes: HashMap<N, NodeIndex>,

    err_not_found: fn(N) -> Error,
    err_no_edge: fn(I, O) -> Error,

    cached_path: Arc<Mutex<HashMap<I, PathToOut<O, N>>>>,
}

impl<I, O, N, E> CachedPath<I, O, N, E>
where
    I: Debug + Eq + Clone + Hash + From<N> + Into<N>,
    O: Debug + Eq + Clone + Hash + From<N> + Into<N>,
    N: Debug + Eq + Clone + Hash,
    E: Debug + Eq + Copy + Default + PartialOrd + Add<Output = E>,
{
    fn new(
        graph: petgraph::Graph<N, E>,
        nodes: HashMap<N, NodeIndex>,
        err_not_found: fn(N) -> Error,
        err_no_edge: fn(I, O) -> Error,
    ) -> Self {
        Self {
            graph,
            nodes,
            cached_path: Arc::new(Mutex::new(HashMap::new())),
            err_not_found,
            err_no_edge,
        }
    }

    fn err_not_found(&self, node: N) -> Error {
        (self.err_not_found)(node)
    }

    fn err_no_edge(&self, node_in: I, node_out: O) -> Error {
        (self.err_no_edge)(node_in, node_out)
    }

    fn node_index(&self, token: N) -> Result<NodeIndex> {
        let &index = self
            .nodes
            .get(&token)
            .ok_or_else(|| self.err_not_found(token))?;
        Ok(index)
    }

    fn update_path(&self, start: I, goal: Option<O>) -> Result<Vec<O>> {
        let log = DEFAULT.new(o!(
            "function" => "CachedPath::update_path",
            "start" => format!("{:?}", start),
            "goal" => format!("{:?}", goal),
        ));
        info!(log, "start");

        let from = self.node_index(start.clone().into())?;
        let to = if let Some(goal) = goal {
            Some(self.node_index(goal.into())?)
        } else {
            None
        };
        let goals = algo::dijkstra(&self.graph, from, to, |e| *e.weight());
        debug!(log, "goals"; "goals" => ?goals);

        let finder = GraphPath {
            graph: self.graph.clone(),
            goals: goals.clone(),
        };

        let paths = finder.find_all_path();
        let mut path_to_outs = HashMap::new();
        let mut outs = Vec::new();
        for mut path in paths.into_iter() {
            if let Some(out) = path.pop() {
                path_to_outs.insert(out.clone().into(), path);
                outs.push(out.into());
            }
        }
        self.cached_path.lock().unwrap().insert(start, path_to_outs);
        Ok(outs)
    }

    fn get_edges(&self, start: I, goal: O) -> Result<Vec<E>> {
        let path = self.get_path(start.clone(), goal.clone())?;
        let mut edges = Vec::new();
        let mut prev = start;
        for token in path.into_iter() {
            let edge = self.get_weight(prev, token.clone().into())?;
            edges.push(edge);
            prev = token.into();
        }
        let edge = self.get_weight(prev, goal)?;
        edges.push(edge);
        Ok(edges)
    }

    fn get_path(&self, start: I, goal: O) -> Result<Vec<N>> {
        let cached_path = self.cached_path.lock().unwrap();
        let path = cached_path
            .get(&start)
            .ok_or(self.err_not_found(start.into()))?
            .get(&goal)
            .ok_or(self.err_not_found(goal.into()))?;
        Ok(path.clone())
    }

    fn get_weight(&self, token_in: I, token_out: O) -> Result<E> {
        let weight: Option<_> = self
            .graph
            .find_edge(
                self.node_index(token_in.clone().into())?,
                self.node_index(token_out.clone().into())?,
            )
            .iter()
            .find_map(|&edge| self.graph.edge_weight(edge).cloned());
        weight.ok_or(self.err_no_edge(token_in, token_out).into())
    }
}

struct GraphPath<N, W> {
    graph: petgraph::Graph<N, W>,
    goals: HashMap<NodeIndex, W>,
}

impl<N, W> GraphPath<N, W>
where
    N: Hash + Eq + Clone,
    W: Eq + Add<Output = W> + Copy,
{
    pub fn find_all_path(&self) -> Vec<Vec<N>> {
        let paths = Rc::new(Mutex::new(HashMap::new()));
        for (&goal, _) in self.goals.iter() {
            self.find_path(Rc::clone(&paths), goal);
        }
        let paths = paths.lock().unwrap();
        let mut results = Vec::new();

        for (_, path) in paths.iter() {
            let path: Vec<N> = path
                .iter()
                .rev()
                .map(|&node_index| self.graph.node_weight(node_index).unwrap().clone())
                .collect();
            results.push(path);
        }

        results
    }

    fn find_path(
        &self,
        locked_paths: Rc<Mutex<HashMap<NodeIndex, Vec<NodeIndex>>>>,
        goal: NodeIndex,
    ) -> Vec<NodeIndex> {
        let log = DEFAULT.new(o!(
            "function" => "GraphPath::find_path",
            "goal" => format!("{:?}", goal),
        ));
        debug!(log, "start");

        if let Some(result) = locked_paths.lock().unwrap().get(&goal) {
            debug!(log, "cached"; "result" => ?result);
            return result.clone();
        }
        let mut path = Vec::new();
        if let Some(prev) = self.find_prev(goal) {
            path.push(goal);
            let more = self.find_path(Rc::clone(&locked_paths), prev);
            path.extend(more);
            let mut paths = locked_paths.lock().unwrap();
            paths.insert(goal, path.clone());
        } else {
            debug!(log, "no previous");
        }
        path
    }

    fn find_prev(&self, target: NodeIndex) -> Option<NodeIndex> {
        let log = DEFAULT.new(o!(
            "function" => "GraphPath::find_prev",
            "target" => format!("{:?}", target),
        ));
        debug!(log, "start");

        self.goals.iter().find_map(|(&node, &d)| {
            if node == target {
                self.graph
                    .edges_directed(node, petgraph::Direction::Incoming)
                    .find_map(|edge| {
                        let source = edge.source();
                        self.goals.get(&source).into_iter().find_map(|&sd| {
                            let x = sd + *edge.weight();
                            (d == x).then_some(source)
                        })
                    })
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod test {
    use crate::ref_finance::path::graph::CachedPath;
    use petgraph::algo::dijkstra;
    use petgraph::graph::NodeIndex;
    use petgraph::Graph;
    use std::collections::HashMap;
    use std::fmt::Debug;
    use std::ops::Add;
    use std::panic;
    use std::thread::sleep;

    #[derive(Default, PartialOrd, Eq, Hash, Copy, Clone)]
    struct Edge<'a> {
        i: &'a str,
        o: &'a str,

        weight: u32,
    }

    impl Add<Edge<'_>> for Edge<'_> {
        type Output = Self;

        fn add(self, rhs: Edge<'_>) -> Self::Output {
            Self {
                i: "",
                o: "",
                weight: self.weight + rhs.weight,
            }
        }
    }

    impl Debug for Edge<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{} <-{}-> {}", self.i, self.weight, self.o)
        }
    }

    impl PartialEq<Self> for Edge<'_> {
        fn eq(&self, other: &Self) -> bool {
            self.weight == other.weight
        }
    }

    impl Ord for Edge<'_> {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            self.weight.cmp(&other.weight)
        }
    }

    #[test]
    fn test_update_path() {
        let mut graph = Graph::new();
        let mut nodes = HashMap::new();
        fn add_node(
            graph: &mut Graph<&'static str, Edge>,
            nodes: &mut HashMap<&'static str, NodeIndex>,
            i: &'static str,
            o: &'static str,
            weight_io: u32,
            weight_oi: u32,
        ) {
            let &mut node_i = nodes.entry(i).or_insert_with(|| graph.add_node(i));
            let &mut node_o = nodes.entry(o).or_insert_with(|| graph.add_node(o));
            graph.add_edge(
                node_i,
                node_o,
                Edge {
                    i,
                    o,
                    weight: weight_io,
                },
            );
            graph.add_edge(
                node_o,
                node_i,
                Edge {
                    i: o,
                    o: i,
                    weight: weight_oi,
                },
            );
        }

        //  A --1|2-- B
        //  |         |
        // 3|2       4|5
        //  |         |
        //  C --4|3-- D
        //  |         |
        // 6|7       8|9
        //  |         |
        //  E --5|6-- F

        // 往路
        // A 1-> B 4-> D 8-> F = 13
        // A 3-> C 6-> E 5-> F = 14
        // A 3-> C 4-> D 8-> F = 15

        // 復路
        // F 9-> D 3-> C 2-> A = 14
        // F 6-> E 7-> C 2-> A = 15
        // F 9-> D 5-> B 2-> A = 16

        add_node(&mut graph, &mut nodes, "A", "B", 1, 2);
        add_node(&mut graph, &mut nodes, "A", "C", 3, 2);
        add_node(&mut graph, &mut nodes, "B", "D", 4, 5);
        add_node(&mut graph, &mut nodes, "C", "D", 4, 3);
        add_node(&mut graph, &mut nodes, "C", "E", 6, 7);
        add_node(&mut graph, &mut nodes, "D", "F", 8, 9);
        add_node(&mut graph, &mut nodes, "E", "F", 5, 6);

        assert_eq!(nodes.len(), 6);
        assert!(nodes.contains_key("A"));

        let cached_path = CachedPath::new(
            graph,
            nodes,
            |node| panic!("not found: {:?}", node),
            |i: &'static str, o: &'static str| panic!("no edge: {:?} -> {:?}", i, o),
        );

        match panic::catch_unwind(|| cached_path.update_path("X", None)) {
            Err(e) => {
                let msg = e.downcast_ref::<String>().unwrap();
                assert_eq!(msg, "not found: \"X\"");
            }
            _ => panic!("should panic"),
        }
        match panic::catch_unwind(|| cached_path.update_path("A", Some("X"))) {
            Err(e) => {
                let msg = e.downcast_ref::<String>().unwrap();
                assert_eq!(msg, "not found: \"X\"");
            }
            _ => panic!("should panic"),
        }
        let goals = cached_path.update_path("A", None).unwrap();
        assert_eq!(goals.len(), 5);
        for goal in goals.into_iter() {
            let gs = cached_path.update_path(goal, Some("A")).unwrap();
            assert!(gs.len() < 6);
        }
    }

    #[test]
    fn test_find_all_path() {
        //     B --2-- C
        //     |       |
        //     3       2
        //     |       |
        //     D       E
        //     |       |
        //     4       7
        //      \     /
        //       F-2-G
        //       |  |
        //       1  3
        //       |  |
        //       H  I
        //       |  |
        //       6  5
        //        \ |
        //         J
        //         |
        //         2
        //         |
        //         A
        let mut graph = Graph::new();
        let a = graph.add_node("A");
        let b = graph.add_node("B");
        let c = graph.add_node("C");
        let d = graph.add_node("D");
        let e = graph.add_node("E");
        let f = graph.add_node("F");
        let g = graph.add_node("G");
        let h = graph.add_node("H");
        let i = graph.add_node("I");
        let j = graph.add_node("J");

        graph.add_edge(c, b, 2);
        graph.add_edge(d, b, 3);
        graph.add_edge(e, c, 2);
        graph.add_edge(f, d, 4);
        graph.add_edge(g, e, 7);
        graph.add_edge(g, f, 2);
        graph.add_edge(h, f, 1);
        graph.add_edge(i, g, 3);
        graph.add_edge(j, h, 6);
        graph.add_edge(j, i, 5);
        graph.add_edge(a, j, 2);

        let goals = dijkstra(&graph, a, None, |e| *e.weight());
        assert_eq!(goals.len(), 10);

        let finder = super::GraphPath {
            graph: graph.clone(),
            goals,
        };
        let mut results = finder.find_all_path();
        assert_eq!(results.len(), 9);
        results.sort();

        assert_eq!(
            results,
            vec![
                vec!["J"],
                vec!["J", "H"],
                vec!["J", "H", "F"],
                vec!["J", "H", "F", "D"],
                vec!["J", "H", "F", "D", "B"],
                vec!["J", "I"],
                vec!["J", "I", "G"],
                vec!["J", "I", "G", "E"],
                vec!["J", "I", "G", "E", "C"],
            ]
        );

        sleep(std::time::Duration::from_secs(1));
    }
}
