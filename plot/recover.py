import polars as pl
import plotly.express as px
import sys

df = pl.scan_ndjson(sys.argv[1])

df = df.filter(
    (
        (pl.col("config").struct["allocator"] == "ralloc")
        & pl.col("config").struct["block"]
        # pl.col("config").struct["allocator"] == "cxlalloc"
    )
)

batch_count = df.select(pl.col("config").struct["batch_count"]).first().collect()

EVERY = "1000000i"

trace = (
    df.select(
        pl.col("config").struct["allocator"].alias("allocator"),
        pl.col("output").struct["trace"].alias("ts"),
        pl.lit(batch_count).alias("delta"),
    )
    .explode("ts")
    .group_by_dynamic(
        "ts",
        every=EVERY,
        group_by="allocator",
    )
    .agg(throughput=pl.col("delta").sum())
    .collect()
    .upsample("ts", every=EVERY, group_by="allocator")
    .with_columns(
        allocator=pl.col("allocator").fill_null(strategy="forward"),
        throughput=pl.col("throughput").fill_null(strategy="zero"),
    )
)

fig = px.line(trace, x="ts", y="throughput", color="allocator")
fig.show()
