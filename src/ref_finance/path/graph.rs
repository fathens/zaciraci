use crate::logging::*;
use crate::ref_finance::errors::Error;
use crate::ref_finance::path::by_token::PoolsByToken;
use crate::ref_finance::path::edge::EdgeWeight;
use crate::ref_finance::pool_info::{PoolInfoList, TokenPair, TokenPairLike};
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use crate::Result;
use anyhow::anyhow;
use petgraph::algo;
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::hash::Hash;
use std::ops::Add;
use std::rc::Rc;
use std::sync::{Arc, Mutex, RwLock};

#[derive(Debug)]
pub struct TokenGraph {
    pools: Arc<PoolInfoList>,
    graph: CachedPath<TokenInAccount, TokenOutAccount, TokenAccount, EdgeWeight>,
}

impl TokenGraph {
    pub fn new(pools_list: Arc<PoolInfoList>) -> Self {
        let pools = Arc::clone(&pools_list);
        let graph = Self::cached_path(pools_list);
        Self { pools, graph }
    }

    fn cached_path(
        pools: Arc<PoolInfoList>,
    ) -> CachedPath<TokenInAccount, TokenOutAccount, TokenAccount, EdgeWeight> {
        let pools_by_token = PoolsByToken::new(pools);
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
        CachedPath::new(graph, nodes)
    }

    pub fn update_graph(&self, start: &TokenInAccount) -> Result<Vec<TokenOutAccount>> {
        let log = DEFAULT.new(o!(
            "function" => "TokenGraph::update_graph",
            "start" => format!("{:?}", start),
        ));
        info!(log, "find goals from start");

        let outs = self.graph.update_path(start, None)?;
        let mut goals = Vec::new();
        for goal in outs.iter() {
            let reversed = self
                .graph
                .update_path(&goal.as_in(), Some(start.as_out()))?;
            if reversed.is_empty() {
                info!(log, "no reversed path found"; "goal" => %goal);
            } else {
                goals.push(goal.clone());
            }
        }
        info!(log, "goals found";
            "outs.count" => %outs.len(),
            "goals.count" => %goals.len(),
        );
        Ok(goals)
    }

    pub fn list_returns(
        &self,
        initial: u128,
        start: &TokenInAccount,
        goals: &[TokenOutAccount],
    ) -> Result<Vec<(TokenOutAccount, u128)>> {
        let log = DEFAULT.new(o!(
            "function" => "TokenGraph::list_returns",
            "initial" => initial,
            "start" => format!("{:?}", start),
        ));
        info!(log, "start");

        let mut returns = HashMap::new();
        for goal in goals.iter() {
            match self.estimate_return(initial, start, goal) {
                Ok(value) => {
                    returns.insert(goal.clone(), value);
                }
                Err(e) => {
                    error!(log, "failed to estimate return";
                        "goal" => %goal,
                        "error" => %e,
                    );
                }
            }
        }
        let mut returns: Vec<_> = returns.into_iter().collect();
        returns.sort_by_key(|(_, value)| *value);
        returns.reverse();
        Ok(returns)
    }

    pub fn estimate_return(
        &self,
        initial: u128,
        start: &TokenInAccount,
        goal: &TokenOutAccount,
    ) -> Result<u128> {
        if initial == 0 {
            return Ok(0);
        }
        let mut value = initial;

        let pairs = self.get_path_with_return(start, goal)?;
        for pair in pairs.iter() {
            value = pair.estimate_return(value)?;
            if value == 0 {
                return Ok(0);
            }
        }

        Ok(value)
    }

    fn get_path(&self, start: &TokenInAccount, goal: &TokenOutAccount) -> Result<Vec<TokenPair>> {
        let mut result = Vec::new();
        let edges = self.graph.get_edges(start, goal)?;
        for edge in edges.iter() {
            let pair_id = edge.pair_id().expect("should be pair id");
            let pair = self.pools.get_pair(pair_id)?;
            result.push(pair);
        }
        Ok(result)
    }

    // 往路と復路のパスを TokenPair のリストで返す
    pub fn get_path_with_return(
        &self,
        start: &TokenInAccount,
        goal: &TokenOutAccount,
    ) -> Result<Vec<TokenPair>> {
        let mut path = self.get_path(start, goal)?;
        path.extend(self.get_path(&goal.as_in(), &start.as_out())?);
        Ok(path)
    }
}

type PathToOut<O, N> = HashMap<O, Vec<N>>;

#[derive(Debug)]
struct CachedPath<I, O, N, E> {
    graph: petgraph::Graph<N, E>,
    nodes: HashMap<N, NodeIndex>,

    cached_path: Arc<RwLock<HashMap<I, PathToOut<O, N>>>>,
}

impl<I, O, N, E> CachedPath<I, O, N, E>
where
    I: Debug + Eq + Clone + Hash + From<N> + Into<N>,
    O: Debug + Eq + Clone + Hash + From<N> + Into<N>,
    N: Debug + Eq + Clone + Hash + Display,
    E: Debug + Eq + Copy + Default + PartialOrd + Add<Output = E>,
{
    fn new(graph: petgraph::Graph<N, E>, nodes: HashMap<N, NodeIndex>) -> Self {
        Self {
            graph,
            nodes,
            cached_path: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn node_index(&self, token: &N) -> Result<NodeIndex> {
        let &index = self.nodes.get(token).ok_or_else(|| {
            let name = token.to_string();
            Error::TokenNotFound(name)
        })?;
        Ok(index)
    }

    fn update_path(&self, start: &I, goal: Option<O>) -> Result<Vec<O>> {
        let log = DEFAULT.new(o!(
            "function" => "CachedPath::update_path",
            "start" => format!("{:?}", start),
            "goal" => format!("{:?}", goal),
        ));
        info!(log, "start");

        let from = self.node_index(&start.clone().into())?;
        let to = if let Some(goal) = goal {
            Some(self.node_index(&goal.into())?)
        } else {
            None
        };
        debug!(log, "finding by dijkstra"; "from" => ?from, "to" => ?to);
        let goals = algo::dijkstra(&self.graph, from, to, |e| *e.weight());
        debug!(log, "goals"; "goals" => ?goals);

        let finder = GraphPath {
            graph: &self.graph,
            goals: &goals,
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
        if path_to_outs.is_empty() {
            info!(log, "no path found");
        } else {
            self.cached_path
                .write()
                .unwrap()
                .insert(start.clone(), path_to_outs);
        }
        Ok(outs)
    }

    fn get_edges(&self, start: &I, goal: &O) -> Result<Vec<E>> {
        let log = DEFAULT.new(o!(
            "function" => "CachedPath::get_edges",
            "start" => format!("{:?}", start),
            "goal" => format!("{:?}", goal),
        ));
        info!(log, "start");
        let path = self.get_path(start, goal)?;
        let mut edges = Vec::new();
        let mut prev = start.clone();
        for token in path.into_iter() {
            let edge = self.get_weight(&prev, &token.clone().into())?;
            edges.push(edge);
            prev = token.into();
        }
        let edge = self.get_weight(&prev, goal)?;
        edges.push(edge);
        Ok(edges)
    }

    fn get_path(&self, start: &I, goal: &O) -> Result<Vec<N>> {
        let cached_path = self.cached_path.read().unwrap();
        let path = cached_path
            .get(start)
            .ok_or_else(|| anyhow!("start token not found: {:?} X-> {:?}", start, goal))?
            .get(goal)
            .ok_or_else(|| anyhow!("goal token not found: {:?} ->X {:?}", start, goal))?;
        Ok(path.clone())
    }

    fn get_weight(&self, token_in: &I, token_out: &O) -> Result<E> {
        let log = DEFAULT.new(o!(
            "function" => "CachedPath::get_weight",
            "token_in" => format!("{:?}", token_in),
            "token_out" => format!("{:?}", token_out),
        ));
        debug!(log, "start");
        let weight: Option<_> = self
            .graph
            .find_edge(
                self.node_index(&token_in.clone().into())?,
                self.node_index(&token_out.clone().into())?,
            )
            .iter()
            .find_map(|&edge| self.graph.edge_weight(edge).cloned());
        weight.ok_or_else(|| anyhow!("invalid edge: {:?} -> {:?}", token_in, token_out))
    }
}

struct GraphPath<'a, N, W> {
    graph: &'a petgraph::Graph<N, W>,
    goals: &'a HashMap<NodeIndex, W>,
}

impl<N, W> GraphPath<'_, N, W>
where
    N: Debug + Eq + Clone + Hash,
    W: Debug + Eq + Copy + Add<Output = W>,
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

        self.goals.get(&target).into_iter().find_map(|&d| {
            debug!(log, "goal"; "d" => format!("{:?}", d));
            self.graph
                .edges_directed(target, petgraph::Direction::Incoming)
                .find_map(|edge| {
                    let source = edge.source();
                    self.goals.get(&source).into_iter().find_map(|&sd| {
                        let x = sd + *edge.weight();
                        (d == x && sd != x).then_some(source)
                    })
                })
        })
    }
}

#[cfg(test)]
mod test {
    use crate::ref_finance::path::edge::EdgeWeight;
    use crate::ref_finance::path::graph::CachedPath;
    use crate::ref_finance::pool_info::TokenPairId;
    use petgraph::algo::dijkstra;
    use petgraph::graph::NodeIndex;
    use petgraph::Graph;
    use std::collections::HashMap;
    use std::fmt::Debug;
    use std::ops::Add;
    use std::panic;

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
            write!(f, "{} {}-> {}", self.i, self.weight, self.o)
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

        let cached_path = CachedPath::new(graph, nodes);

        match panic::catch_unwind(|| cached_path.update_path(&"X", None)) {
            Err(e) => panic!("something wrong: {:?}", e),
            Ok(Ok(v)) => panic!("should error: {:?}", v),
            Ok(Err(e)) => {
                let msg = format!("{}", e);
                assert_eq!(msg, "Cannot find token account: X");
            }
        }
        match panic::catch_unwind(|| cached_path.update_path(&"A", Some("X"))) {
            Err(e) => panic!("something wrong: {:?}", e),
            Ok(Ok(v)) => panic!("should error: {:?}", v),
            Ok(Err(e)) => {
                let msg = format!("{}", e);
                assert_eq!(msg, "Cannot find token account: X");
            }
        }
        let goals = cached_path.update_path(&"A", None).unwrap();
        assert_eq!(goals.len(), 5);
        for goal in goals.into_iter() {
            let gs = cached_path.update_path(&goal, Some("A")).unwrap();
            assert!(gs.len() < 6);
            assert!(gs.len() > 0);
        }

        // A <-> B
        assert_eq!(
            format!("{:?}", cached_path.get_edges(&"A", &"B").unwrap()),
            "[A 1-> B]"
        );
        assert_eq!(
            format!("{:?}", cached_path.get_edges(&"B", &"A").unwrap()),
            "[B 2-> A]"
        );

        // A <-> C
        assert_eq!(
            format!("{:?}", cached_path.get_edges(&"A", &"C").unwrap()),
            "[A 3-> C]"
        );
        assert_eq!(
            format!("{:?}", cached_path.get_edges(&"C", &"A").unwrap()),
            "[C 2-> A]"
        );

        // A <-> D
        assert_eq!(
            format!("{:?}", cached_path.get_edges(&"A", &"D").unwrap()),
            "[A 1-> B, B 4-> D]"
        );
        assert_eq!(
            format!("{:?}", cached_path.get_edges(&"D", &"A").unwrap()),
            "[D 3-> C, C 2-> A]"
        );

        // A <-> E
        assert_eq!(
            format!("{:?}", cached_path.get_edges(&"A", &"E").unwrap()),
            "[A 3-> C, C 6-> E]"
        );
        assert_eq!(
            format!("{:?}", cached_path.get_edges(&"E", &"A").unwrap()),
            "[E 7-> C, C 2-> A]"
        );

        // A <-> F
        assert_eq!(
            format!("{:?}", cached_path.get_edges(&"A", &"F").unwrap()),
            "[A 1-> B, B 4-> D, D 8-> F]"
        );
        assert_eq!(
            format!("{:?}", cached_path.get_edges(&"F", &"A").unwrap()),
            "[F 9-> D, D 3-> C, C 2-> A]"
        );
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
            graph: &graph,
            goals: &goals,
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
    }

    #[test]
    fn test_find_all_path_looped() {
        fn weight(v: u8) -> EdgeWeight {
            EdgeWeight::new(
                TokenPairId {
                    pool_id: 0,
                    token_in: 0.into(),
                    token_out: 0.into(),
                },
                1,
                v as u128,
            )
        }
        //  B-1-C
        //  |   |
        //  2   2
        //   \ /
        //    A
        let mut graph = Graph::new();
        let a = graph.add_node("A");
        let b = graph.add_node("B");
        let c = graph.add_node("C");

        graph.add_edge(a, b, weight(2));
        graph.add_edge(a, c, weight(2));
        graph.add_edge(b, c, weight(1));
        graph.add_edge(c, b, weight(1));

        let goals = dijkstra(&graph, a, None, |e| *e.weight());
        assert_eq!(goals.len(), 3);

        let finder = super::GraphPath {
            graph: &graph,
            goals: &goals,
        };
        let mut results = finder.find_all_path();
        assert_eq!(results.len(), 2);
        results.sort();

        assert_eq!(results, vec![vec!["B"], vec!["C"]]);
    }
}
