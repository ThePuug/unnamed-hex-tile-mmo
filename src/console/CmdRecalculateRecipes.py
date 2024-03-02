import argparse
import cmd2
import math
import sqlite3
from cmd2 import CommandSet
from itertools import groupby
from functools import reduce
from logging import debug

DATABASE = "development.db"
SQL = "./sql/recipe-decompose.sql"

class CmdRecalculateRecipes(CommandSet):

    def __init__(self):
        super().__init__()

    parser_recalculate_recipes = cmd2.Cmd2ArgumentParser()
    parser_recalculate_recipes.add_argument('type', choices=['decomposer'])

    @cmd2.as_subcommand_to('recalculate','recipes',parser_recalculate_recipes)
    def recalculate_recipes(self, ns: argparse.Namespace):
        if ns.type == 'decomposer': self.implRecalculateRecipesDecomposer()

    def implRecalculateRecipesDecomposer(self):
        sql = open(SQL,"r").read()
        connection = sqlite3.connect(DATABASE)
        connection.row_factory = sqlite3.Row
        result = [dict(row) for row in connection.execute(sql).fetchall()]
        for key,val0 in groupby(result, lambda r: [r["facility"],r["option"],r["input"],r["element_group"]]):
            val = list(val0)
            size_remaining = val[0]["input_size"]
            debug("{} sz:{}".format(key,val[0]["input_size"]))
            for idx,component in enumerate(val):
                n = 0
                sum = lambda agg, it: agg + math.floor(((component["output_num_min"]+n+1)/component["output_num_min"]) * it["output_num_min"]) * it["output_size"]
                while(size_remaining >= reduce(sum, val[idx:], 0)): n+=1
                component["output_num"] = component["output_num_min"]+n
                size_remaining -= component["output_num"] * component["output_size"]
                debug("  \u2937 {} ({}) sz:{}".format(component["output"],component["output_num"],component["output_size"]))
                connection.execute("""
                                   update recipe_decomposer 
                                   set output_num=:output_num 
                                   where facility=:facility 
                                   and option=:option 
                                   and input=:input
                                   and element_group=:element_group""", component)
            assert(size_remaining==0)
        connection.commit()

