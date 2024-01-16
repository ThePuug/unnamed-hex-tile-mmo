drop table if exists recipe_decomposer;
create table recipe_decomposer(
    facility string not null,
    input string not null,
    element_group string not null,
    output string not null,
    option int default 1,
    priority_override int default NULL,
    output_num int default 1,
    primary key (input,element_group,facility,output,option),
    foreign key (input) references component(name),
    foreign key (facility) references facility(name),
    foreign key (output) references component(name)
);
delete from recipe_decomposer;
insert into recipe_decomposer
values('cutter','bar','inorganic','rod',1,NULL,NULL)
    ,('cutter','bar','inorganic','dust',1,NULL,NULL)
    ,('grinder','bar','inorganic','chip',1,NULL,NULL)
    ,('grinder','bar','inorganic','dust',1,NULL,NULL)
    ,('heater','bar','inorganic','molten alloy',1,NULL,NULL)
    ,('cutter','beam','inorganic','bar',1,NULL,NULL)
    ,('cutter','beam','inorganic','dust',1,NULL,NULL)
    ,('grinder','beam','inorganic','chip',1,NULL,NULL)
    ,('grinder','beam','inorganic','dust',1,NULL,NULL)
    ,('heater','beam','inorganic','molten alloy',1,NULL,NULL)
    ,('grinder','block','inorganic','chip',1,NULL,NULL)
    ,('grinder','block','inorganic','dust',1,NULL,NULL)
    ,('heater','block','inorganic','molten alloy',1,NULL,NULL)
    ,('cutter','block','organic','stick',1,NULL,NULL)
    ,('cutter','block','organic','dust',1,NULL,NULL)
    ,('grinder','chip','inorganic','dust',1,NULL,NULL)
    ,('heater','chip','inorganic','molten alloy',1,NULL,NULL)
    ,('cutter','chunk','inorganic','block',1,NULL,NULL)
    ,('cutter','chunk','inorganic','chip',1,NULL,NULL)
    ,('cutter','chunk','inorganic','dust',1,NULL,NULL)
    ,('grinder','chunk','inorganic','chip',1,NULL,NULL)
    ,('grinder','chunk','inorganic','dust',1,NULL,NULL)
    ,('heater','chunk','inorganic','molten alloy',1,NULL,NULL)
    ,('heater','dust','inorganic','molten alloy',1,NULL,NULL)
    ,('heater','ingot','inorganic','molten alloy',1,NULL,NULL)
    ,('cutter','log','organic','plank',1,NULL,NULL)
    ,('cutter','log','organic','stick',1,NULL,NULL)
    ,('cutter','log','organic','dust',1,NULL,NULL)
    ,('cutter','log','organic','block',2,NULL,NULL)
    ,('cutter','log','organic','stick',2,NULL,NULL)
    ,('cutter','log','organic','dust',2,NULL,NULL)
    ,('cooler','molten alloy','inorganic','slag',1,NULL,NULL)
    ,('cutter','plank','organic','stick',1,NULL,NULL)
    ,('cutter','plank','organic','dust',1,NULL,NULL)
    ,('grinder','rod','inorganic','chip',1,NULL,NULL)
    ,('grinder','rod','inorganic','dust',1,NULL,NULL)
    ,('heater','rod','inorganic','molten alloy',1,NULL,NULL)
    ,('grinder','slag','inorganic','dust',1,NULL,NULL)
    ,('heater','slag','inorganic','molten alloy',1,NULL,NULL)
    ,('cutter','stick','organic','dust',1,NULL,NULL)
    ,('grinder','tile','inorganic','chip',1,NULL,NULL)
    ,('grinder','tile','inorganic','dust',1,NULL,NULL)
    ,('heater','tile','inorganic','molten alloy',1,NULL,NULL)
;

drop table if exists facility_group;
create table facility_group(
    name string not null,
    primary key (name)
);
insert into facility_group 
values('decomposer')
;

drop table if exists facility;
create table facility(
    name string not null,
    facility_group string not null,
    primary key (name),
    foreign key (facility_group) references facility_group(name)
);
insert into facility 
values('heater','decomposer')
    ,('cutter','decomposer')
    ,('grinder','decomposer')
;

drop table if exists element_group;
create table element_group(
    name string not null,
    primary key (name)
);
insert into element_group 
values('organic')
    ,('inorganic')
;

drop table if exists element;
create table element(
    name string not null,
    element_group string not null,
    primary key (name),
    foreign key (element_group) references element_group(name)
);
insert into element 
values('carbonium','organic')
    ,('nutrium','organic')
    ,('gleamium','inorganic')
    ,('dullium','inorganic')
;

drop table if exists component;
create table component(
    name string not null,
    size int not null,
    element_group string not null,
    primary key (name,element_group),
    FOREIGN key (element_group) references element_group(name)
);
insert into component
values('chunk',120,'inorganic')
    ,('beam',42,'inorganic')
    ,('block',36,'inorganic')
    ,('tile',12,'inorganic')
    ,('bar',18,'inorganic')
    ,('ingot',18,'inorganic')
    ,('rod',8,'inorganic')
    ,('chip',3,'inorganic')
    ,('dust',1,'inorganic')
    ,('molten alloy',1,'inorganic')
    ,('slag',1,'inorganic')
    ,('log',360,'organic')
    ,('plank',42,'organic')
    ,('block',36,'organic')
    ,('stick',8,'organic')
    ,('dust',1,'organic')
;
