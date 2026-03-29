-- trade_transactions: RESTRICT → CASCADE
ALTER TABLE trade_transactions
  DROP CONSTRAINT fk_trade_transactions_evaluation_period;
ALTER TABLE trade_transactions
  ADD CONSTRAINT fk_trade_transactions_evaluation_period
  FOREIGN KEY (evaluation_period_id)
  REFERENCES evaluation_periods(period_id)
  ON DELETE CASCADE;

-- portfolio_holdings: RESTRICT → CASCADE
ALTER TABLE portfolio_holdings
  DROP CONSTRAINT portfolio_holdings_evaluation_period_id_fkey;
ALTER TABLE portfolio_holdings
  ADD CONSTRAINT portfolio_holdings_evaluation_period_id_fkey
  FOREIGN KEY (evaluation_period_id)
  REFERENCES evaluation_periods(period_id)
  ON DELETE CASCADE;
