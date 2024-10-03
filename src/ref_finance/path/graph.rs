use crate::logging::*;
use crate::ref_finance::errors::Error;
use crate::ref_finance::path::by_token::PoolsByToken;
use crate::ref_finance::path::edge::EdgeWeight;
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use crate::Result;
use near_primitives::num_rational::BigRational;
use petgraph::algo;
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

type PathToOut = HashMap<TokenOutAccount, Vec<TokenAccount>>;

#[derive(Debug)]
#[allow(dead_code)]
pub struct TokenGraph {
    graph: petgraph::Graph<TokenAccount, EdgeWeight>,
    nodes: HashMap<TokenAccount, NodeIndex>,

    cached_path: Arc<Mutex<HashMap<TokenInAccount, PathToOut>>>,
}

#[allow(dead_code)]
impl TokenGraph {
    pub fn new(pools_by_token: PoolsByToken) -> Self {
        let mut graph = petgraph::Graph::new();
        let pools = pools_by_token.tokens();
        let mut nodes = HashMap::new();
        for token_in in pools {
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
            graph,
            nodes,
            cached_path: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn list_returns(
        &self,
        start: TokenInAccount,
    ) -> Result<HashMap<TokenOutAccount, BigRational>> {
        let goals = self.update_path(start.clone(), None)?;
        for goal in goals.iter() {
            self.update_path(goal.as_in(), Some(start.as_out()))?;
        }

        let mut returns = HashMap::new();
        for goal in goals.into_iter() {
            let recto = self.estimate_return(start.clone(), goal.clone())?;
            let verso = self.estimate_return(
                goal.as_account().clone().into(),
                start.as_account().clone().into(),
            )?;
            returns.insert(goal, recto * verso);
        }
        Ok(returns)
    }

    fn estimate_return(&self, start: TokenInAccount, goal: TokenOutAccount) -> Result<BigRational> {
        let path = self.get_path(start.clone(), goal.clone())?;

        let mut value = EdgeWeight::default().to_rational();

        let mut prev = start;
        for token in path.iter() {
            value *= self.get_weight(prev, token.clone().into())?.to_rational();
            prev = token.clone().into();
        }
        value *= self.get_weight(prev, goal.clone())?.to_rational();

        Ok(value)
    }

    fn node_index(&self, token: TokenAccount) -> Result<NodeIndex> {
        let &index = self.nodes.get(&token).ok_or(Error::TokenNotFound(token))?;
        Ok(index)
    }

    fn update_path(
        &self,
        start: TokenInAccount,
        goal: Option<TokenOutAccount>,
    ) -> Result<Vec<TokenOutAccount>> {
        let log = DEFAULT.new(o!(
            "function" => "TokenGraph::list_asymmetric_path",
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

    fn get_path(&self, start: TokenInAccount, goal: TokenOutAccount) -> Result<Vec<TokenAccount>> {
        let cached_path = self.cached_path.lock().unwrap();
        let path = cached_path
            .get(&start)
            .ok_or(Error::TokenNotFound(start.as_account().clone()))?
            .get(&goal)
            .ok_or(Error::TokenNotFound(goal.as_account().clone()))?;
        Ok(path.clone())
    }

    fn get_weight(
        &self,
        token_in: TokenInAccount,
        token_out: TokenOutAccount,
    ) -> Result<EdgeWeight> {
        let weight: Option<_> = self
            .graph
            .find_edge(
                self.node_index(token_in.clone().into())?,
                self.node_index(token_out.clone().into())?,
            )
            .iter()
            .find_map(|&edge| self.graph.edge_weight(edge).cloned());
        weight.ok_or(Error::NoValidEddge(token_in, token_out).into())
    }
}

struct GraphPath<N, W> {
    graph: petgraph::Graph<N, W>,
    goals: HashMap<NodeIndex, W>,
}

impl<N, W> GraphPath<N, W>
where
    N: std::hash::Hash + Eq + Clone,
    W: Eq + std::ops::Add<Output = W> + Copy,
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
    use petgraph::algo::dijkstra;
    use petgraph::Graph;
    use std::thread::sleep;

    #[test]
    fn test_find_path() {
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
