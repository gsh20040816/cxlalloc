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

THROUGHPUT = "output/throughput"
MAX_RSS = "output/resource_usage/max_rss"


def main():
    alt.renderers.enable("browser")

    df = pl.scan_ndjson(sys.argv[1])
    df = (
        unnest_all(df, "/")
        .sort("allocator", "config_benchmark/trace", "config_global/thread_count")
        .filter(pl.col("config_global/thread_count").is_in([1, 4, 16, 32]))
        .with_columns(
            pl.when(FILTER_MEMCACHED)
            .then(0)
            .otherwise(pl.col(THROUGHPUT))
            .alias(THROUGHPUT),
            pl.when(FILTER_MEMCACHED).then(0).otherwise(pl.col(MAX_RSS)).alias(MAX_RSS),
        )
    )

    baseline = (
        df.filter(pl.col("allocator") == "cxlalloc")
        .select(THROUGHPUT, MAX_RSS)
        .collect()
    )

    df = (
        df.group_by("allocator")
        .agg(
            trace=pl.col("config_benchmark/trace"),
            thread_count=pl.col("config_global/thread_count"),
            throughput=pl.col(THROUGHPUT),
            throughput_relative=pl.col(THROUGHPUT) / baseline.get_column(THROUGHPUT),
            max_rss=pl.col(MAX_RSS),
            max_rss_relative=pl.col(MAX_RSS) / baseline.get_column(MAX_RSS),
        )
        .explode(
            [
                "trace",
                "thread_count",
                "throughput",
                "throughput_relative",
                "max_rss",
                "max_rss_relative",
            ]
        )
        .collect()
    )

    tput = plot(df, "throughput", True)
    rss = plot(df, "max_rss", False)

    chart = alt.vconcat(
        tput,
        rss,
        title=alt.Title("Memcached Workloads", anchor="middle"),
    )

    chart.show()


def plot(df, metric: str, top: bool):
    y = alt.Y(metric).axis(format="s", title=None)

    if metric == "max_rss":
        y = y.scale(alt.Scale(domain=[0, 14 * 2**30], clamp=True))

    base = alt.Chart(df, width=alt.Step(10)).encode(
        x=alt.X("thread_count:N", title=None if top else "Thread Count").axis(
            labels=False if top else True
        ),
        y=y,
        xOffset=alt.XOffset("allocator", sort=SORT),
    )

    bar = base.mark_bar().encode(color=alt.Color("allocator", sort=SORT))

    absolute = (
        base.transform_filter(alt.datum.allocator == "cxlalloc")
        .mark_text(align="left", angle=270, dx=5)
        .encode(text=alt.Text(metric, format=".2s"))
    )

    relative = (
        base.transform_filter(alt.datum.allocator != "cxlalloc")
        .transform_calculate(
            text=alt.expr.if_(
                alt.datum[metric] > 0,
                alt.expr.format(alt.datum[metric + "_relative"], ".2r") + "x",
                "X",
            ),
        )
        .mark_text(align="left", angle=270, dx=5)
        .encode(
            text="text:N",
            color=alt.condition(
                alt.datum[metric] > 0,
                alt.value("black"),
                alt.value("red"),
            ),
        )
    )

    chart = (bar + absolute + relative).properties(
        height=100 if metric == "max_rss" else 200
    )
    column = alt.Column(
        "trace",
        header=alt.Header(title=metric, titleOrient="left", labels=top),
    )

    return chart.facet(column=column).resolve_scale(y="independent")


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
