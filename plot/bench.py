import sys
import polars as pl
import plotly.express as px


def main():
    df = pl.read_ndjson(sys.argv[1])

    fig = None
    if "load" in sys.argv[1]:
        fig = plot_load(df)
    else:
        fig = plot_insert_proportion(df)

    fig.write_image("plot.svg")
    fig.show()


def plot_load(df):
    df = (
        df.group_by("allocator", "control", "ycsb", "index")
        .agg(
            index_inline=pl.col("ycsb").struct["index_inline"].first(),
            time_mean=pl.col("time").mean() / 1e6,
            time_std=pl.col("time").std() / 1e6,
        )
        .with_columns(
            thread_total=pl.col("control").struct["process_count"]
            * pl.col("control").struct["thread_count"]
        )
        .sort("allocator", "thread_total")
    )

    fig = px.line(
        df,
        x="thread_total",
        y="time_mean",
        error_y="time_std",
        color="allocator",
        facet_col="index_inline",
        facet_row="index",
        markers=True,
    )

    fig.update_xaxes(title_text="Thread Count", tickvals=df["thread_total"].unique())
    fig.update_yaxes(title_text="Time (s)", autorangeoptions_include=0.0)
    return fig


def plot_insert_proportion(df):
    df = (
        df.group_by("allocator", "index", "control", "ycsb")
        .agg(
            pl.col("ycsb").struct["insert_proportion"].first(),
            pl.col("ycsb").struct["index_inline"].first(),
            time_mean=pl.col("time").max() / 1e6,
            time_std=pl.col("time").std() / 1e6,
        )
        .sort("allocator", "insert_proportion")
    )

    fig = px.line(
        df,
        x="insert_proportion",
        y="time_mean",
        error_y="time_std",
        color="allocator",
        facet_col="index_inline",
        facet_row="index",
        markers=True,
    )

    fig.update_xaxes(
        title_text="Insert Proportion", tickvals=df["insert_proportion"].unique()
    )
    fig.update_yaxes(title_text="Time (s)", autorangeoptions_include=0.0)
    return fig


if __name__ == "__main__":
    main()
