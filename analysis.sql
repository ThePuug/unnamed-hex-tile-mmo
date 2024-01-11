select *
from element
where 1
order by name;

select * 
from element_group
where 1
order by name;

select *
from component
where 1
order by element_group, size desc;

select *
from facility f, facility_group fg
where 1
and f.facility_group=fg.name
order by fg.name, f.name;

select facility
    , rd.element_group
    , rd.option
    , rd.input
    , cin.size input_size
    , output
    , output_num
    , cout.size output_size
from recipe_decomposer rd
join component cin on (cin.name=rd.input and cin.element_group = rd.element_group)
join component cout on (cout.name=rd.output and cout.element_group=cin.element_group)
where 1
-- and input = 'block'
-- and facility = 'cutter'
-- order by rd.input,rd.facility,cout.size desc;
order by rd.facility,rd.element_group,rd.option,rd.input,cout.size desc;

select *
from recipe_decomposer
where input='block' and facility='cutter';