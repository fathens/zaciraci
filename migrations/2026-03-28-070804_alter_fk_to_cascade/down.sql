-- trade_transactions: CASCADE → RESTRICT
ALTER TABLE trade_transactions
  DROP CONSTRAINT fk_trade_transactions_evaluation_period;
ALTER TABLE trade_transactions
  ADD CONSTRAINT fk_trade_transactions_evaluation_period
  FOREIGN KEY (evaluation_period_id)
  REFERENCES evaluation_periods(period_id);

-- portfolio_holdings: CASCADE → RESTRICT
ALTER TABLE portfolio_holdings
  DROP CONSTRAINT portfolio_holdings_evaluation_period_id_fkey;
ALTER TABLE portfolio_holdings
  ADD CONSTRAINT portfolio_holdings_evaluation_period_id_fkey
  FOREIGN KEY (evaluation_period_id)
  REFERENCES evaluation_periods(period_id);
