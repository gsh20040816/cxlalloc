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

BASELINE = "cxlalloc"
ABSOLUTE = "absolute"
RELATIVE = "relative"

FILTER_MEMCACHED = (pl.col(ALLOC) == "cxl_shm") & pl.col(TRACE).is_in(
    [
        "12",
        "37",
    ]
)


def main():
    alt.renderers.enable("browser")

    df = pl.scan_ndjson(sys.argv[1])

    # Reshape data
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
        df.filter(pl.col(ALLOC) == BASELINE).select(THROUGHPUT, MAX_RSS).collect()
    )

    # Compute relative metrics
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

    outer = []

    height = 100

    for row, (absolute, relative) in enumerate(
        [(THROUGHPUT, THROUGHPUT_RELATIVE), (MAX_RSS, MAX_RSS_RELATIVE)]
    ):
        inner = []

        for col, trace in enumerate(df.get_column(TRACE).unique(maintain_order=True)):
            data = df.filter(pl.col(TRACE) == trace).select(
                cs.by_name(ALLOC, THREAD_COUNT),
                pl.col(absolute).alias(ABSOLUTE),
                pl.col(relative).alias(RELATIVE),
            )

            # RSS has one outlier and otherwise similar values
            # Clamp outlier and focus on reasonable range
            y = alt.Y(ABSOLUTE).axis(format="s", title=None)
            if absolute == MAX_RSS:
                cutoff = (
                    data.filter(pl.col(ALLOC) == BASELINE).select(ABSOLUTE).max().item()
                    * 1.7
                )
                y = y.scale(alt.Scale(domain=[0, cutoff], clamp=True))

            title = ""
            if row == 0:
                title = alt.Title("Cluster " + trace)

            base = alt.Chart(data, width=alt.Step(10), title=title).encode(
                x=alt.X(THREAD_COUNT + ":N", title=None).axis(
                    alt.Axis(labels=row == 1)
                ),
                y=y,
                xOffset=alt.XOffset(ALLOC, sort=SORT),
            )

            chart = base.mark_bar().encode(
                color=alt.Color(ALLOC, sort=SORT)
            ) + annotate(base)

            inner.append(chart.properties(width=alt.Step(10), height=height))

        outer.append(
            alt.hconcat(
                *inner,
                title=alt.Title(
                    absolute, orient="left", align="center", anchor="middle"
                ),
            )
        )

    outer.append(
        alt.hconcat(
            *[],
            title=alt.Title(
                "Thread Count", orient="bottom", align="center", anchor="middle"
            ),
        )
    )

    chart = (
        alt.vconcat(
            *outer,
            center=True,
            title=alt.Title("Memcached Workload", align="center", anchor="middle"),
        )
        .configure_concat(spacing=5)
        .configure_legend(
            orient="none",
            direction="horizontal",
            legendX=0,
            # HACK: need to manually set position to force overlap
            legendY=height * 2.75,
            titleOrient="left",
        )
    )

    chart.save("memcached.json")
    chart.show()


def annotate(base):
    absolute = (
        base.transform_filter(alt.datum[ALLOC] == "cxlalloc")
        .mark_text(align="left", angle=270, dx=5)
        .encode(text=alt.Text(ABSOLUTE, format=".2s"))
    )

    relative = (
        base.transform_filter(alt.datum[ALLOC] != "cxlalloc")
        .transform_calculate(
            text=alt.expr.if_(
                alt.datum[ABSOLUTE] > 0,
                alt.expr.format(alt.datum[RELATIVE], ".2r") + "x",
                "X",
            ),
        )
        .mark_text(align="left", angle=270, dx=5)
        .encode(
            text="text:N",
            color=alt.condition(
                alt.datum[ABSOLUTE] > 0,
                alt.value("black"),
                alt.value("red"),
            ),
        )
    )

    return absolute + relative


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
