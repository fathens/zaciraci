create table if not exists pool_info (
    id int primary key,
    pool_kind varchar not null,
    token_account_ids text[] not null,
    amounts numeric(40)[] not null,
    total_fee bigint not null,
    shares_total_supply numeric(40) not null,
    amp numeric(20) not null,
    updated_at timestamp not null
);
