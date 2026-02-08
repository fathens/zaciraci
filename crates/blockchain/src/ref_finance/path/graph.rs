use crate::Result;
use crate::ref_finance::path::by_token::PoolsByToken;
use crate::ref_finance::path::edge::EdgeWeight;
use anyhow::anyhow;
use common::types::TokenAccount;
use common::types::{TokenInAccount, TokenOutAccount};
use dex::errors::Error;
use dex::{PoolInfoList, TokenPath};
use logging::*;
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
        let log = DEFAULT.new(o!("function" => "cached_path"));
        info!(log, "start building token graph");

        let pools_by_token = PoolsByToken::new(pools);
        let mut graph = petgraph::Graph::new();
        let mut nodes = HashMap::new();
        for token_in in pools_by_token.tokens() {
            let node = graph.add_node(token_in.clone());
            nodes.insert(token_in, node);
        }
        info!(log, "nodes created"; "count" => nodes.len());

        let mut edges_added = 0;
        let mut edges_skipped = 0;

        for (token_in, &node_in) in nodes.iter() {
            let token_in_str = token_in.to_string();
            let is_important = token_in_str.contains("akaia")
                || token_in_str.contains("a0b86991")
                || token_in_str.contains("wrap.near");

            let token_in_account = token_in.to_in();
            let edges_by_token_out = pools_by_token.get_groups_by_out(&token_in_account);

            for (token_out, edges) in edges_by_token_out.iter() {
                let token_out_str = token_out.to_string();
                let is_important_pair = is_important
                    || token_out_str.contains("akaia")
                    || token_out_str.contains("a0b86991")
                    || token_out_str.contains("wrap.near");

                let at_top = edges.at_top();

                if at_top.is_none() {
                    edges_skipped += 1;
                    if is_important_pair {
                        warn!(log, "at_top() returned None";
                            "token_in" => token_in_str.as_str(),
                            "token_out" => token_out_str.as_str(),
                        );
                    }
                } else {
                    for edge in at_top.into_iter() {
                        for &node_out in nodes.get(token_out.inner()).into_iter() {
                            graph.add_edge(node_in, node_out, edge.weight());
                            edges_added += 1;

                            if is_important_pair {
                                trace!(log, "edge added";
                                    "token_in" => token_in_str.as_str(),
                                    "token_out" => token_out_str.as_str(),
                                    "weight" => format!("{:?}", edge.weight()),
                                );
                            }
                        }
                    }
                }
            }
        }

        trace!(log, "graph construction complete";
            "nodes" => nodes.len(),
            "edges_added" => edges_added,
            "edges_skipped" => edges_skipped,
        );

        CachedPath::new(graph, nodes)
    }

    #[cfg(test)]
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
        trace!(log, "start");

        let out = self.graph.update_path(start, Some(goal.clone()))?;
        Ok(out.contains(goal))
    }

    pub fn update_graph(&self, start: &TokenInAccount) -> Result<Vec<TokenOutAccount>> {
        let log = DEFAULT.new(o!(
            "function" => "TokenGraph::update_graph",
            "start" => format!("{:?}", start),
        ));
        trace!(log, "find goals from start");

        let outs = self.graph.update_path(start, None)?;
        let mut goals = Vec::new();
        for goal in outs.iter() {
            let reversed = self
                .graph
                .update_path(&goal.as_in(), Some(start.as_out()))?;
            if reversed.is_empty() {
                trace!(log, "no reversed path found"; "goal" => %goal);
            } else {
                goals.push(goal.clone());
            }
        }
        trace!(log, "goals found";
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

    /// 各トークンの推定値とパス情報を取得
    ///
    /// list_values と同様だが、各トークンのスワップパス情報も返す。
    pub fn list_values_with_path(
        &self,
        initial: u128,
        start: &TokenInAccount,
        goals: &[TokenOutAccount],
    ) -> Result<Vec<(TokenOutAccount, u128, TokenPath)>> {
        let log = DEFAULT.new(o!(
            "function" => "TokenGraph::list_values_with_path",
            "initial" => initial,
            "start" => format!("{:?}", start),
        ));

        let mut values = Vec::new();
        for goal in goals.iter() {
            match self.get_path(start, goal) {
                Ok(path) => match path.calc_value(initial) {
                    Ok(value) => {
                        values.push((goal.clone(), value, path));
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
        values.sort_by_key(|(_, value, _)| std::cmp::Reverse(*value));
        Ok(values)
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
        trace!(log, "start");

        // キャッシュチェック (goal が None の場合のみ)
        if goal.is_none() {
            let cached = self.cached_path.read().unwrap();
            if let Some(path_to_outs) = cached.get(start) {
                trace!(log, "cache hit, returning cached result"; "outs_count" => path_to_outs.len());
                return Ok(path_to_outs.keys().cloned().collect());
            }
        }

        let from = self.node_index(&start.clone().into())?;
        let to = if let Some(goal) = goal {
            Some(self.node_index(&goal.into())?)
        } else {
            None
        };
        trace!(log, "finding by dijkstra"; "from" => ?from, "to" => ?to);
        let goals = algo::dijkstra(&self.graph, from, to, |e| *e.weight());
        trace!(log, "dijkstra complete"; "reachable_count" => goals.len());

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
            trace!(log, "no path found");
        } else {
            self.cached_path
                .write()
                .unwrap()
                .insert(start.clone(), path_to_outs);
        }
        Ok(outs)
    }

    fn get_edges(&self, start: &I, goal: &O) -> Result<Vec<E>> {
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
        if let Some(result) = locked_paths.lock().unwrap().get(&goal) {
            return result.clone();
        }
        let mut path = Vec::new();
        if let Some(prev) = self.find_prev(goal) {
            path.push(goal);
            let more = self.find_path(Rc::clone(&locked_paths), prev);
            path.extend(more);
            let mut paths = locked_paths.lock().unwrap();
            paths.insert(goal, path.clone());
        }
        path
    }

    fn find_prev(&self, target: NodeIndex) -> Option<NodeIndex> {
        self.goals.get(&target).into_iter().find_map(|&d| {
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
