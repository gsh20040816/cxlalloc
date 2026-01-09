import common
import polars as pl
from polars import selectors as cs

df = common.scan_ndjson()
df = common.collapse(
    df,
)
df = (
    df.drop(
        cs.starts_with(common.SWCC),
        cs.starts_with(common.HWCC),
        cs.starts_with(common.PSS),
        cs.starts_with(common.THROUGHPUT),
        common.PROCESS_COUNT,
    )
    .filter(pl.col(common.ALLOCATOR) != common.Allocator.CXLALLOC_NONRECOVERABLE)
    .group_by(cs.exclude(common.TIME))
    .agg(pl.col(common.TIME).mean())
    .sort(common.WORKLOAD, common.ALLOCATOR, common.THREAD_COUNT)
    .select(common.WORKLOAD, common.ALLOCATOR, common.THREAD_COUNT, common.TIME)
    .with_columns(pl.col(common.TIME).cast(pl.Decimal(scale=1)))
    .collect()
)

total = df.select(pl.col(common.TIME).sum()).item()
partial = (
    df.filter(
        pl.col(common.ALLOCATOR).is_in(
            [common.Allocator.BOOST, common.Allocator.LIGHTNING]
        )
    )
    .select(pl.col(common.TIME).sum())
    .item()
)

print(f"Total time (h) = {total / 60 / 60:.1f}")
print(
    f"Boost/lightning time (h) = {partial / 60 / 60:.1f} ({partial * 100 / total:.1f}%)"
)

df.write_csv("estimate.csv")
