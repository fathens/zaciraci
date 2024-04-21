create table if not exists counter (value int not null default 0);

create table if not exists pool (
    `index` int primary key,
    kind text not null,
    token_a text not null,
    token_b text not null,
    amount_a text not null,
    amount_b text not null,
    total_fee text not null,
    shares_total_supply text not null,
    amp text not null,
    updated_at timestamp not null default now()
);