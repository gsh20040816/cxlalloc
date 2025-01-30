import sys
import polars as pl
import polars.selectors as cs
import polars.datatypes as dt
import plotly.graph_objects as go

SCHEMA = {
    "ts": dt.UInt64,
    "heap": dt.Categorical(),
    "name": dt.Categorical(),
    "thread": dt.UInt16(),
    "class": dt.UInt64(),
    "size": dt.Int64(),
}

df = pl.read_csv(
    sys.argv[1],
    has_header=False,
    schema=SCHEMA,
)

min_ts = df.select("ts").min()

df = df.select(
    pl.col("ts").sub(min_ts),
    ~cs.by_name("ts"),
).filter(pl.col("thread").is_null())

fig = go.Figure()

for name in df["name"].unique():
    data = df.filter((pl.col("heap") == "small") & (pl.col("name") == name))
    fig.add_trace(go.Scatter(x=data["ts"] / 1e6, y=data["size"], name=name))

fig.show()
