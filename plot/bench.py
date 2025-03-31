import sys
import polars as pl
import altair as alt

SORT = [
    "cxlalloc",
    "mimalloc",
    "ralloc",
    "cxl_shm",
    "boost",
    "lightning",
]

FILTER_MEMCACHED = (pl.col("allocator") == "cxl_shm") & pl.col(
    "config_benchmark/trace"
).is_in(
    [
        "./twitter/cluster12.000.parquet",
        "./twitter/cluster37.000.parquet",
    ]
)


def main():
    alt.renderers.enable("browser")

    df = pl.scan_ndjson(sys.argv[1])
    df = (
        unnest_all(df, "/")
        .sort("allocator", "config_benchmark/trace", "config_global/thread_count")
        .with_columns(
            pl.when(FILTER_MEMCACHED)
            .then(0)
            .otherwise(pl.col("output/throughput"))
            .alias("output/throughput")
        )
    )

    baseline = (
        df.filter(pl.col("allocator") == "cxlalloc")
        .select("output/throughput")
        .collect()
        .to_series(0)
    )

    df = (
        df.group_by("allocator")
        .agg(
            trace=pl.col("config_benchmark/trace"),
            thread_count=pl.col("config_global/thread_count"),
            throughput=pl.col("output/throughput"),
            throughput_relative=pl.col("output/throughput") / baseline,
        )
        .explode(["trace", "thread_count", "throughput", "throughput_relative"])
        .collect()
    )

    base = alt.Chart(df).encode(
        x="thread_count:N",
        y=alt.Y("throughput").axis(format="s"),
        xOffset=alt.XOffset("allocator", sort=SORT),
    )

    bar = base.mark_bar().encode(color=alt.Color("allocator", sort=SORT))

    absolute = (
        base.transform_filter(alt.datum.allocator == "cxlalloc")
        .mark_text(align="left", angle=270, dx=5)
        .encode(text=alt.Text("throughput", format=".2s"))
    )

    relative = (
        base.transform_filter(alt.datum.allocator != "cxlalloc")
        .transform_calculate(
            text=alt.expr.if_(
                alt.datum.throughput > 0,
                alt.expr.format(alt.datum.throughput_relative, ".2f") + "x",
                "X",
            ),
        )
        .mark_text(align="left", angle=270, dx=5)
        .encode(
            text="text:N",
            color=alt.condition(
                alt.datum.throughput > 0,
                alt.value("black"),
                alt.value("red"),
            ),
        )
    )

    (bar + absolute + relative).facet(row="trace").resolve_scale(y="independent").show()


# https://github.com/pola-rs/polars/issues/12353
def unnest_all(df, separator="."):
    def _unnest_all(schema, separator):
        def _unnest(schema, path=[]):
            for name, dtype in schema.items():
                base_type = dtype.base_type()

                if base_type == pl.Struct:
                    yield from _unnest(dtype.to_schema(), path + [name])
                else:
                    yield path + [name], dtype

        for (col, *fields), dtype in _unnest(schema):
            expr = pl.col(col)

            for field in fields:
                expr = expr.struct[field]

            if col == "":
                name = separator.join(fields)
            else:
                name = separator.join([col] + fields)

            yield expr.alias(name)

    return df.select(_unnest_all(df.schema, separator))


if __name__ == "__main__":
    main()
