import sys
import polars as pl
import polars.selectors as cs
import altair as alt

SORT = [
    "cxlalloc",
    "mimalloc",
    "ralloc",
    "cxl_shm",
    "boost",
    "lightning",
]

THROUGHPUT = "Throughput"
THROUGHPUT_RELATIVE = "Relative Throughput"
MAX_RSS = "Max RSS"
MAX_RSS_RELATIVE = "Relative Max RSS"
ALLOC = "Allocator"
TRACE = "Trace"
THREAD_COUNT = "Thread Count"

FILTER_MEMCACHED = (pl.col(ALLOC) == "cxl_shm") & pl.col(TRACE).is_in(
    [
        "12",
        "37",
    ]
)


def main():
    alt.renderers.enable("browser")

    df = pl.scan_ndjson(sys.argv[1])
    df = (
        unnest_all(df, "/")
        .select(
            pl.col("allocator").alias(ALLOC),
            pl.col("config_benchmark/trace")
            .str.strip_prefix("./twitter/cluster")
            .str.strip_suffix(".000.parquet")
            .alias(TRACE),
            pl.col("config_global/thread_count").alias(THREAD_COUNT),
            pl.col("output/throughput").alias(THROUGHPUT),
            pl.col("output/resource_usage/max_rss").alias(MAX_RSS),
        )
        .filter(pl.col(THREAD_COUNT).is_in([1, 4, 16, 32]))
        .sort(ALLOC, TRACE, THREAD_COUNT)
        .with_columns(
            pl.when(FILTER_MEMCACHED)
            .then(0)
            .otherwise(pl.col(THROUGHPUT))
            .alias(THROUGHPUT),
            pl.when(FILTER_MEMCACHED).then(0).otherwise(pl.col(MAX_RSS)).alias(MAX_RSS),
        )
    )

    baseline = (
        df.filter(pl.col(ALLOC) == "cxlalloc").select(THROUGHPUT, MAX_RSS).collect()
    )

    df = (
        df.group_by(ALLOC)
        .agg(
            cs.by_name(TRACE, THREAD_COUNT, THROUGHPUT, MAX_RSS),
            pl.col(THROUGHPUT)
            .truediv(baseline.get_column(THROUGHPUT))
            .alias(THROUGHPUT_RELATIVE),
            pl.col(MAX_RSS)
            .truediv(baseline.get_column(MAX_RSS))
            .alias(MAX_RSS_RELATIVE),
        )
        .explode(cs.exclude(ALLOC))
        .collect()
    )

    tput = plot(df, THROUGHPUT, True)
    rss = plot(df, MAX_RSS, False)

    chart = (
        alt.vconcat(
            tput,
            rss,
            title=alt.Title("Memcached Workloads", anchor="middle"),
            spacing=0.0,
        )
        .configure_facet(spacing=0.0)
        .configure_legend(
            orient="none",
            legendX=450,
            legendY=380,
            direction="horizontal",
            titleOrient="left",
        )
    )

    chart.save("memcached.json")
    chart.show()


def plot(df, metric: str, top: bool):
    y = alt.Y(metric).axis(format="s", title=None)

    if metric == MAX_RSS:
        y = y.scale(alt.Scale(domain=[0, 14 * 2**30], clamp=True, nice=False))

    base = alt.Chart(df, width=alt.Step(10)).encode(
        x=alt.X(THREAD_COUNT + ":N", title=None if top else THREAD_COUNT).axis(
            # HACK: Why is there an offset?
            offset=-6,
            labels=False if top else True,
        ),
        y=y,
        xOffset=alt.XOffset(ALLOC, sort=SORT),
    )

    bar = base.mark_bar().encode(color=alt.Color(ALLOC, sort=SORT))

    absolute = (
        base.transform_filter(alt.datum[ALLOC] == "cxlalloc")
        .mark_text(align="left", angle=270, dx=5)
        .encode(text=alt.Text(metric, format=".2s"))
    )

    relative = (
        base.transform_filter(alt.datum[ALLOC] != "cxlalloc")
        .transform_calculate(
            text=alt.expr.if_(
                alt.datum[metric] > 0,
                alt.expr.format(alt.datum["Relative " + metric], ".2r") + "x",
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
        height=100 if metric == MAX_RSS else 200
    )
    column = alt.Column(
        TRACE,
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
