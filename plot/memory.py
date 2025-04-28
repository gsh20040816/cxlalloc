import sys
import polars as pl
import polars.selectors as cs
import polars.datatypes as dt
import plotly.express as px

SCHEMA = {
    "ts": dt.Int64,
    "heap": dt.Categorical(),
    "name": dt.Categorical(),
    "thread": dt.UInt16(),
    "class": dt.UInt64(),
    "size": dt.Int64(),
}

RESOLUTION_TIME_MICRO = 1000
RESOLUTION_SPACE = 2**10


def main():
    df = downsample(
        pl.scan_csv(
            sys.argv[1],
            has_header=False,
            schema=SCHEMA,
        ).sort(by=pl.col("ts")),
        interval=RESOLUTION_TIME_MICRO,
    )

    min_ts = df.select("ts").min().collect().item()

    df = df.select(
        pl.col("ts").sub(min_ts),
        ~cs.by_name("ts"),
    )

    integral = (
        df.group_by("name", "thread")
        .agg(
            pl.col("ts") // RESOLUTION_TIME_MICRO,
            pl.col("size").cum_sum() // RESOLUTION_SPACE,
        )
        .explode("ts", "size")
        .sort("ts")
        .collect()
    )

    fig = px.line(
        integral, x="ts", y="size", color="name", facet_row="name", facet_col="thread"
    )
    fig.show()
    fig.write_html("trace-integral.html", include_plotlyjs="cdn", include_mathjax=False)

    # fig = px.line(df.collect(), x="ts", y="size", color="name")
    # fig.show()
    # fig.write_html("trace-derivative.html", include_plotlyjs="cdn", include_mathjax=False)


def downsample(df, interval=1000):
    return (
        df.group_by_dynamic(
            "ts", group_by=cs.exclude("ts", "size"), every=f"{interval}i"
        )
        .agg(pl.col("size").sum())
        .sort("ts")
    )


if __name__ == "__main__":
    main()
