-- Ref Finance flow collected from the NearBlocks API (crates/ref_flow).
-- Three tables: a receipt index (append-only), a per-transaction enrichment
-- marker that doubles as the coverage denominator, and the executed trades
-- reconstructed from each Ref receipt. All times are UTC without time zone,
-- matching token_rates / pool_info.

-- 1. Every swap / ft_on_transfer receipt listed for v2.ref-finance.near,
--    successful or not. receipt_id is the natural key (base58 CryptoHash).
CREATE TABLE ref_flow_receipts (
    receipt_id    VARCHAR   PRIMARY KEY,
    tx_hash       VARCHAR   NOT NULL,
    block_height  BIGINT    NOT NULL CHECK (block_height >= 0),
    block_time    TIMESTAMP NOT NULL,
    predecessor   VARCHAR   NOT NULL,
    -- Action method names of the receipt in order (e.g. {swap} or
    -- {swap,withdraw}). NULL elements are rejected because `ALL`/`ANY`
    -- comparisons silently pass over them.
    methods       TEXT[]    NOT NULL
        CHECK (cardinality(methods) BETWEEN 1 AND 16
           AND array_position(methods, NULL) IS NULL),
    ok            BOOLEAN   NOT NULL
);
CREATE INDEX idx_ref_flow_receipts_block_time ON ref_flow_receipts (block_time);
CREATE INDEX idx_ref_flow_receipts_tx_hash    ON ref_flow_receipts (tx_hash);

-- 2. One row per successful transaction seen in ref_flow_receipts. Created
--    together with the receipts, so the set of rows is the full population
--    ("frame") and enriched_at marks which of them have been fetched from
--    /v3/txns/{hash}/receipts. Coverage of a time bucket is
--    count(enriched_at IS NOT NULL) / count(*).
CREATE TABLE ref_flow_txs (
    tx_hash      VARCHAR   PRIMARY KEY,
    -- Time of the transaction's first Ref receipt; used for bucketing,
    -- retention and the pending scan.
    block_time   TIMESTAMP NOT NULL,
    enriched_at  TIMESTAMP NULL,
    -- Failed enrichment attempts. The collector isolates a transaction
    -- after a fixed number of failures by setting enriched_at with
    -- trade_count = 0 so it stops consuming API budget.
    attempts     SMALLINT  NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    trade_count  SMALLINT  NULL CHECK (trade_count >= 0),
    CHECK ((enriched_at IS NULL) = (trade_count IS NULL))
);
CREATE INDEX idx_ref_flow_txs_block_time ON ref_flow_txs (block_time);
-- Pending scan: newest un-enriched transactions first.
CREATE INDEX idx_ref_flow_txs_pending ON ref_flow_txs (block_time DESC)
    WHERE enriched_at IS NULL;

-- 3. Executed trades, one per Ref receipt. Hops are stored as parallel
--    arrays (pool_ids / hop_tokens_in / hop_tokens_out / fee_bps) whose
--    lengths are forced equal; token_in / token_out are redundant copies
--    of the first / last hop kept for indexing and are pinned by CHECKs.
CREATE TABLE ref_flow_trades (
    receipt_id     VARCHAR   PRIMARY KEY,
    tx_hash        VARCHAR   NOT NULL REFERENCES ref_flow_txs (tx_hash) ON DELETE CASCADE,
    block_height   BIGINT    NOT NULL CHECK (block_height >= 0),
    block_time     TIMESTAMP NOT NULL,
    -- Economic sender: the receipt predecessor for direct swaps, the
    -- transfer's sender_id for ft_on_transfer.
    sender         VARCHAR   NOT NULL,
    -- SYNC: allowed values must match TradeRoute::from_str in
    -- crates/dex/src/ref_trade.rs
    route          VARCHAR   NOT NULL CHECK (route IN ('swap', 'ft')),
    pool_ids       INTEGER[] NOT NULL,
    hop_tokens_in  TEXT[]    NOT NULL,
    hop_tokens_out TEXT[]    NOT NULL,
    -- Pool fee per hop in units of 1/10000 (PoolInfo.total_fee), taken from
    -- the latest pool_info row at or before block_time. NULL as a whole when
    -- the fee of any hop is unknown; never partially filled.
    fee_bps        INTEGER[] NULL,
    token_in       VARCHAR   NOT NULL,
    -- Amounts in the token's smallest units. u128::MAX is 39 digits.
    amount_in      NUMERIC(39, 0) NOT NULL
        CHECK (amount_in > 0
           AND amount_in <= 340282366920938463463374607431768211455),
    token_out      VARCHAR   NOT NULL,
    amount_out     NUMERIC(39, 0) NOT NULL
        CHECK (amount_out >= 0
           AND amount_out <= 340282366920938463463374607431768211455),
    CHECK (cardinality(pool_ids) BETWEEN 1 AND 16
       AND cardinality(hop_tokens_in)  = cardinality(pool_ids)
       AND cardinality(hop_tokens_out) = cardinality(pool_ids)
       AND array_position(pool_ids, NULL) IS NULL
       AND array_position(hop_tokens_in, NULL) IS NULL
       AND array_position(hop_tokens_out, NULL) IS NULL),
    CHECK (token_in  = hop_tokens_in[1]),
    CHECK (token_out = hop_tokens_out[cardinality(hop_tokens_out)]),
    CHECK (fee_bps IS NULL
        OR (cardinality(fee_bps) = cardinality(pool_ids)
        AND array_position(fee_bps, NULL) IS NULL
        AND 0 <= ALL(fee_bps)
        AND 10000 > ALL(fee_bps)))
);
CREATE INDEX idx_ref_flow_trades_block_time ON ref_flow_trades (block_time);
-- FK side index: ON DELETE CASCADE from ref_flow_txs and tx_hash lookups.
CREATE INDEX idx_ref_flow_trades_tx_hash    ON ref_flow_trades (tx_hash);

-- Daily retention DELETE removes ~1/90 of each table, and the pending
-- partial index makes enriched_at updates non-HOT, so vacuum early.
ALTER TABLE ref_flow_receipts SET (
  autovacuum_vacuum_scale_factor = 0.02,
  autovacuum_vacuum_threshold = 10000
);
ALTER TABLE ref_flow_txs SET (
  autovacuum_vacuum_scale_factor = 0.01,
  autovacuum_vacuum_threshold = 5000
);
ALTER TABLE ref_flow_trades SET (
  autovacuum_vacuum_scale_factor = 0.02,
  autovacuum_vacuum_threshold = 10000
);
