create table if not exists counter (value int not null default 0);

create table if not exists pool_info (
    pool_id int,
    body jsonb not null,
    updated_at timestamp not null default now(),
    primary key (pool_id)
);