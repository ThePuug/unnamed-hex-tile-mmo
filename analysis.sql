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

select input
    , cin.size input_size
    , output
    , cout.size output_size
    , facility
from recipe_decomposer rd
join component cin on (cin.name=rd.input)
join component cout on (cout.name=rd.output and cout.element_group=cin.element_group)
where 1
and cin.element_group = cout.element_group
and input = 'block'
and facility = 'cutter'
-- order by rd.input,rd.facility,cout.size desc;
order by rd.facility,rd.input,cout.size desc;

select *
from recipe_decomposer
where input='block' and facility='cutter';