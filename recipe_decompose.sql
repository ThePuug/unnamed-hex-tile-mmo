select input
    , facility
    , option
    , input_size
    , output
    , min_num_output
    , min_num_output * output_size as total_output_size
    , input_size - min_num_output * output_size as residual_size
    , t.*
from (select rd.input
        , rd.element_group
        , rd.option
        , rd.output
        , rd.facility
        , cin.size input_size
        , cout.size output_size
        , agg.sum
        , IFNULL(rd.priority_override,cout.size) priority
        , CAST(IFNULL(rd.priority_override,cout.size) as REAL) / agg.sum as ratio
        , cast(cin.size * CAST(IFNULL(rd.priority_override,cout.size) as REAL) / agg.sum as INTEGER) as ideal_output
        , cast(cin.size * CAST(IFNULL(rd.priority_override,cout.size) as REAL) / agg.sum as INTEGER) / cout.size as min_num_output
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
) t
order by facility, option, input, output_size desc;
