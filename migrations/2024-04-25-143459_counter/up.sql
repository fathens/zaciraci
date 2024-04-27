create table if not exists counter (
    id serial primary key,
    value int not null default 0
);
