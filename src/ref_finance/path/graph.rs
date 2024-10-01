use crate::logging::*;
use crate::ref_finance::errors::Error;
use crate::ref_finance::path::by_token::PoolsByToken;
use crate::ref_finance::path::edge::EdgeWeight;
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use crate::Result;
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

    pub fn list_asymmetric_path(&self, from: TokenAccount) -> Result<Vec<TokenAccount>> {
        let log = DEFAULT.new(o!(
            "function" => "TokenGraph::list_asymmetric_path",
            "from" => format!("{:?}", from),
        ));
        info!(log, "start");

        let &start = self.nodes.get(&from).ok_or(Error::TokenNotFound(from))?;
        let goals = algo::dijkstra(&self.graph, start, None, |e| *e.weight());
        debug!(log, "goals"; "goals" => ?goals);

        let finder = GraphPath {
            graph: self.graph.clone(),
            goals: goals.clone(),
        };

        let _ = finder.find_all_path();

        todo!()
    }
}

struct GraphPath<N, W> {
    graph: petgraph::Graph<N, W>,
    goals: HashMap<NodeIndex, W>,
}

impl<N, W> GraphPath<N, W>
where
    W: Eq + std::ops::Add<Output = W> + Copy,
{
    fn find_all_path(&self) -> HashMap<NodeIndex, Vec<NodeIndex>> {
        let paths = Rc::new(Mutex::new(HashMap::new()));
        for (&goal, _) in self.goals.iter() {
            self.find_path(Rc::clone(&paths), goal);
        }
        let paths = paths.lock().unwrap().clone();
        paths
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
    use petgraph::graph::NodeIndex;
    use petgraph::visit::IntoNodeReferences;
    use petgraph::Graph;
    use std::collections::HashMap;
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
        let paths = finder.find_all_path();

        fn get_node<'a>(graph: &Graph<&'a str, i32>, index: NodeIndex) -> &'a str {
            graph
                .node_references()
                .find_map(|(i, &node)| (i == index).then_some(node))
                .expect("node not found")
        }
        assert_eq!(paths.len(), 9);

        let mut results = HashMap::new();
        for (node_index, path) in paths {
            let node = get_node(&graph, node_index);
            let path: Vec<_> = path
                .iter()
                .rev()
                .map(|&node_index| get_node(&graph, node_index))
                .collect();
            results.insert(node, path);
        }

        assert_eq!(results["J"], vec!["J"]);
        assert_eq!(results["H"], vec!["J", "H"]);
        assert_eq!(results["F"], vec!["J", "H", "F"]);
        assert_eq!(results["D"], vec!["J", "H", "F", "D"]);
        assert_eq!(results["B"], vec!["J", "H", "F", "D", "B"]);
        assert_eq!(results["I"], vec!["J", "I"]);
        assert_eq!(results["G"], vec!["J", "I", "G"]);
        assert_eq!(results["E"], vec!["J", "I", "G", "E"]);
        assert_eq!(results["C"], vec!["J", "I", "G", "E", "C"]);

        sleep(std::time::Duration::from_secs(1));
    }
}
