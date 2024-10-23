#![deny(warnings)]

mod config;
mod cron;
mod errors;
mod logging;
mod persistence;
mod ref_finance;
mod wallet;
mod web;

use bigdecimal::BigDecimal;
use errors::Error;
use num_bigint::BigUint;
type Result<T> = std::result::Result<T, Error>;

#[tokio::main]
async fn main() {
    use logging::*;

    let log = DEFAULT.new(o!("function" => "main"));
    info!(log, "Starting up");
    debug!(log, "log level check");
    trace!(log, "log level check");
    error!(log, "log level check");
    warn!(log, "log level check");
    crit!(log, "log level check");

    let a = 1_u8;
    let b = BigUint::from(2_u8);
    let c = &BigDecimal::from(3_u8);

    debug!(log, "details";
      "a" => a,
      "b" => %b,
      "c" => %c,
    );

    let x = b + 1_u8;
    let y = c + 1_u8;
    debug!(log, "details";
      "x" => %x,
      "y" => %y,
    );

    let base = wallet::WALLET.derive(0).unwrap();
    let account_zero = base.derive(0).unwrap();
    info!(log, "Account 0 created"; "pubkey" => %account_zero.pub_base58());

    tokio::spawn(cron::run());
    web::run().await
}
