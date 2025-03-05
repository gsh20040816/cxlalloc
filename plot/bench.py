import sys
import polars as pl
import plotly.express as px


def main():
    df = pl.read_ndjson(sys.argv[1])

    df = (
        df.group_by("benchmark", "allocator", "process_count", "thread_count")
        .agg(pl.col("time").max() / 1e6)
        .sort("benchmark", "process_count", "thread_count")
        .with_columns(thread_total=pl.col("process_count") * pl.col("thread_count"))
    )

    fig = px.line(
        df,
        x="thread_total",
        y="time",
        color="allocator",
        facet_col="process_count",
        facet_row="benchmark",
        markers=True,
    )

    fig.update_xaxes(title_text="Thread Count", tickvals=df["thread_total"].unique())
    fig.update_yaxes(title_text="Time (s)")
    fig.show()


if __name__ == "__main__":
    main()
