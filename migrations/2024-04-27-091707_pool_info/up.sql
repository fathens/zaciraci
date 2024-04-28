create table if not exists pool_info (
    id int primary key,
    pool_kind varchar not null,
    token_account_id_a varchar(64) not null,
    token_account_id_b varchar(64) not null,
    amount_a numeric(40) not null,
    amount_b numeric(40) not null,
    total_fee bigint not null,
    shares_total_supply numeric(40) not null,
    amp numeric(20) not null,
    updated_at timestamp not null
);
