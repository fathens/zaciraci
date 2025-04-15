use crate::Result;
use crate::logging::*;
use crate::ref_finance::errors::Error;
use crate::ref_finance::path::by_token::PoolsByToken;
use crate::ref_finance::path::edge::EdgeWeight;
use crate::ref_finance::pool_info::PoolInfoList;
use crate::ref_finance::pool_info::TokenPath;
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
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

    pub fn update_single_path(
        &self,
        start: &TokenInAccount,
        goal: &TokenOutAccount,
    ) -> Result<bool> {
        let log = DEFAULT.new(o!(
            "function" => "TokenGraph::update_single_path",
            "start" => format!("{:?}", start),
            "goal" => format!("{:?}", goal),
        ));
        info!(log, "start");

        let out = self.graph.update_path(start, Some(goal.clone()))?;
        Ok(out.contains(goal))
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
        self.list_estimated_values(initial, start, goals, true)
    }

    pub fn list_values(
        &self,
        initial: u128,
        start: &TokenInAccount,
        goals: &[TokenOutAccount],
    ) -> Result<Vec<(TokenOutAccount, u128)>> {
        self.list_estimated_values(initial, start, goals, false)
    }

    fn list_estimated_values(
        &self,
        initial: u128,
        start: &TokenInAccount,
        goals: &[TokenOutAccount],
        with_return: bool,
    ) -> Result<Vec<(TokenOutAccount, u128)>> {
        let log = DEFAULT.new(o!(
            "function" => "TokenGraph::list_estimated_values",
            "initial" => initial,
            "start" => format!("{:?}", start),
        ));

        let mut values = HashMap::new();
        for goal in goals.iter() {
            let res_path = if with_return {
                self.get_path_with_return(start, goal)
            } else {
                self.get_path(start, goal)
            };
            match res_path {
                Ok(path) => match path.calc_value(initial) {
                    Ok(value) => {
                        values.insert(goal.clone(), value);
                    }
                    Err(e) => {
                        error!(log, "failed to estimate value";
                            "goal" => %goal,
                            "error" => %e,
                        );
                    }
                },
                Err(e) => {
                    error!(log, "failed to get path";
                        "start" => %start,
                        "goal" => %goal,
                        "error" => %e,
                    );
                }
            }
        }
        let mut values: Vec<_> = values.into_iter().collect();
        values.sort_by_key(|(_, value)| *value);
        values.reverse();
        Ok(values)
    }

    pub fn get_path(&self, start: &TokenInAccount, goal: &TokenOutAccount) -> Result<TokenPath> {
        let mut result = Vec::new();
        let edges = self.graph.get_edges(start, goal)?;
        for edge in edges.iter() {
            let pair_id = edge.pair_id().expect("should be pair id");
            let pair = self.pools.get_pair(pair_id)?;
            result.push(pair);
        }
        Ok(TokenPath(result))
    }

    // 往路と復路のパスを TokenPair のリストで返す
    pub fn get_path_with_return(
        &self,
        start: &TokenInAccount,
        goal: &TokenOutAccount,
    ) -> Result<TokenPath> {
        let mut path = self.get_path(start, goal)?;
        path.0
            .extend(self.get_path(&goal.as_in(), &start.as_out())?.0);
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
mod tests;
