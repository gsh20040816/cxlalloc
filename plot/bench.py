from pathlib import PurePath
import os
import subprocess as sp
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
ALLOCATOR = "Allocator"
WORKLOAD = "Workload"
THREAD_COUNT = "Thread Count"

BASELINE = "cxlalloc"
ABSOLUTE = "absolute"
RELATIVE = "relative"

FILTER_MEMCACHED = (pl.col(ALLOCATOR) == "cxl_shm") & (
    pl.col(WORKLOAD).str.starts_with("Cluster 12")
    | pl.col(WORKLOAD).str.starts_with("Cluster 37")
)


def main():
    alt.renderers.enable("browser")

    path = sys.argv[1]

    df = pl.scan_ndjson(path)
    title_workload = None
    translate = None

    if "memcached" in path:
        title_workload = "Memcached Trace"
        translate = pl.concat_str(
            pl.lit("Cluster "),
            pl.col("config_benchmark/trace")
            .str.strip_prefix("twitter/cluster")
            .str.strip_suffix(".000.parquet"),
        )
    elif "ycsb" in path:
        title_workload = "YCSB Workload"
        translate = (
            pl.when(pl.col("config_benchmark/name") == "ycsb_load")
            .then(pl.lit("Load"))
            .otherwise(pl.lit("YCSB-D"))
        )
    elif "microbenchmark" in path:
        title_workload = "Microbenchmark"
        translate = (
            pl.when(pl.col("config_benchmark/name") == "thread_test")
            .then(pl.lit("Threadtest"))
            .otherwise(pl.lit("Xmalloc"))
        )

    else:
        raise Exception(f"Unhandled benchmark: {path}")

    df = reshape(df, translate)
    bl = baseline(df)

    # Compute relative metrics
    df = (
        df.group_by(ALLOCATOR)
        .agg(
            cs.by_name(WORKLOAD, THREAD_COUNT, THROUGHPUT, MAX_RSS),
            pl.col(THROUGHPUT)
            .truediv(bl.get_column(THROUGHPUT))
            .alias(THROUGHPUT_RELATIVE),
            pl.col(MAX_RSS).truediv(bl.get_column(MAX_RSS)).alias(MAX_RSS_RELATIVE),
        )
        .explode(cs.exclude(ALLOCATOR))
        .collect()
    )

    workloads = df.get_column(WORKLOAD).unique(maintain_order=True)

    outer = []

    height = 100

    for row, (absolute, relative) in enumerate(
        [(THROUGHPUT, THROUGHPUT_RELATIVE), (MAX_RSS, MAX_RSS_RELATIVE)]
    ):
        inner = []

        for col, workload in enumerate(workloads):
            data = df.filter(pl.col(WORKLOAD) == workload).select(
                cs.by_name(ALLOCATOR, THREAD_COUNT),
                pl.col(absolute).alias(ABSOLUTE),
                pl.col(relative).alias(RELATIVE),
            )

            # RSS has one outlier and otherwise similar values
            # Clamp outlier and focus on reasonable range
            y = alt.Y(ABSOLUTE).axis(format="s", title=None)
            if absolute == MAX_RSS:
                cutoff = (
                    data.filter(pl.col(ALLOCATOR) == BASELINE)
                    .select(ABSOLUTE)
                    .max()
                    .item()
                    * 1.7
                )
                y = y.scale(alt.Scale(domain=[0, cutoff], clamp=True))

            header_workload = ""
            if row == 0:
                header_workload = alt.Title(workload)

            title_x = ""
            if row == 1 and col == len(workloads) - 1:
                title_x = THREAD_COUNT

            base = alt.Chart(data, width=alt.Step(10), title=header_workload).encode(
                x=alt.X(THREAD_COUNT + ":N", title=title_x).axis(
                    alt.Axis(labels=row == 1)
                ),
                y=y,
                xOffset=alt.XOffset(ALLOCATOR, sort=SORT),
            )

            chart = base.mark_bar().encode(
                color=alt.Color(ALLOCATOR, sort=SORT)
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

    # outer.append(
    #     alt.hconcat(
    #         *[],
    #         title=alt.Title(
    #             "Thread Count", orient="bottom", align="right", anchor="middle"
    #         ),
    #     )
    # )

    chart = (
        alt.vconcat(
            *outer,
            center=True,
            title=alt.Title(title_workload, align="center", anchor="middle"),
        )
        .configure_concat(spacing=5)
        .configure_legend(
            orient="none",
            direction="horizontal",
            legendX=0,
            # HACK: need to manually set position to force overlap
            legendY=height * 2.66,
            titleOrient="left",
        )
    )

    path = PurePath(path)
    json = f"{path.stem}.json"
    pdf = f"{path.stem}.pdf"

    # Export to PDF
    chart.save(json)
    sp.run(
        [
            PurePath(os.environ.get("HOME"), ".cargo", "bin", "vl-convert"),
            "vl2pdf",
            "--input",
            json,
            "--output",
            pdf,
        ]
    )
    os.remove(json)

    chart.show()


def baseline(df):
    return (
        df.filter(pl.col(ALLOCATOR) == BASELINE).select(THROUGHPUT, MAX_RSS).collect()
    )


def reshape(df, workload):
    return (
        unnest_all(df, "/")
        .select(
            pl.col("allocator").alias(ALLOCATOR),
            pl.col("config_global/thread_count").alias(THREAD_COUNT),
            workload.alias(WORKLOAD),
            pl.col("output/throughput").alias(THROUGHPUT),
            pl.col("output/resource_usage/max_rss").alias(MAX_RSS),
        )
        .filter(pl.col(THREAD_COUNT).is_in([1, 4, 16, 32]))
        .with_columns(
            # Filter out cxl-shm for certain workloads
            pl.when(FILTER_MEMCACHED)
            .then(pl.struct(pl.lit(0).alias(THROUGHPUT), pl.lit(0).alias(MAX_RSS)))
            .otherwise(pl.struct(THROUGHPUT, MAX_RSS))
            .struct.rename_fields([THROUGHPUT, MAX_RSS])
            .struct.unnest(),
        )
        .sort(ALLOCATOR, WORKLOAD, THREAD_COUNT)
    )


def annotate(base):
    absolute = (
        base.transform_filter(alt.datum[ALLOCATOR] == BASELINE)
        .mark_text(align="left", angle=270, dx=5)
        .encode(text=alt.Text(ABSOLUTE, format=".2s"))
    )

    relative = (
        base.transform_filter(alt.datum[ALLOCATOR] != BASELINE)
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

    return df.select(_unnest_all(df.collect_schema(), separator))


if __name__ == "__main__":
    main()
