select rd.facility
    , rd.option 
    , rd.input
    , cin.size input_size
    , rd.element_group
    , rd.output
    , cout.size output_size
    , agg.sum
    , IFNULL(rd.priority_override,cout.size) priority
    , CAST(IFNULL(rd.priority_override,cout.size) as REAL) / agg.sum as output_ratio
    , cast(cin.size * CAST(IFNULL(rd.priority_override,cout.size) as REAL) / agg.sum as INTEGER) as output_size_ideal
    , cast(cin.size * CAST(IFNULL(rd.priority_override,cout.size) as REAL) / agg.sum as INTEGER) / cout.size as output_num_min
from recipe_decomposer rd
join component cin on (cin.name=rd.input and cin.element_group=rd.element_group)
join component cout on (cout.name=rd.output and cout.element_group=cin.element_group)
left join (
    select rd.facility
        , rd.input
        , rd.element_group
        , rd.option
        , SUM(IFNULL(rd.priority_override,cout.size)) sum
    from recipe_decomposer rd
    join component cin on (cin.name=rd.input and cin.element_group=rd.element_group)
    join component cout on (cout.name=rd.output and cout.element_group=cin.element_group)
    group by rd.facility,rd.input,rd.element_group,rd.option
) agg on (agg.facility=rd.facility and agg.input=rd.input and agg.element_group=rd.element_group and agg.option=rd.option)
order by rd.facility, rd.option, rd.input, cout.size desc;
