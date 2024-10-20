use crate::logging::*;
use crate::ref_finance::path;
use crate::ref_finance::pool_info::PoolInfoList;
use crate::ref_finance::token_account::{TokenInAccount, TokenOutAccount};

pub fn run_swap(
    pools: PoolInfoList,
    start: TokenInAccount,
    goal: TokenOutAccount,
    initial: u128,
) -> crate::Result<u128> {
    let log = DEFAULT.new(o!(
        "function" => "run_swap",
        "start" => format!("{}", start),
        "goal" => format!("{}", goal),
        "initial" => initial,
    ));
    let path = path::swap_path(pools, start.clone(), goal.clone())?;
    trace!(log, "path"; "path" => format!("{:?}", path));
    todo!("run_swap")
}
