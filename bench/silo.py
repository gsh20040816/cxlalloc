import subprocess
import os

# https://stackoverflow.com/questions/5137497/find-the-current-directory-and-files-directory
ROOT = os.path.dirname(os.path.realpath(__file__))
TIME = 30

ALLOCATORS = ["cxlalloc", "cxl-shm", "ralloc", "mimalloc"]
THREADS = [1, 10, 20, 30, 40]
WORKLOADS = {
    "a": [50, 50, 0, 0],
    "b": [95, 5, 0, 0],
}


def main():
    for allocator in ALLOCATORS:
        for node in [1, 2]:
            for workload in list(WORKLOADS.keys()) + [None]:
                for thread_count in THREADS:
                    run(allocator, node, thread_count, 40, workload)

                # Vary scale factor, fix threads
                for scale_factor in THREADS:
                    run(allocator, node, 40, scale_factor, workload)


def run(
    allocator: str, numa_node: int, thread_count: int, scale_factor: int, workload: str
):
    benchmark = "tpcc" if workload is None else "ycsb"
    name = "tpcc" if workload is None else f"ycsb-{workload}"
    path = (
        f"{ROOT}/{name}-{allocator}-n{numa_node}-t{thread_count}-sf{scale_factor}.log"
    )

    print(
        f"Running {name} with {allocator}, n{numa_node}, t{thread_count}, sf{scale_factor}"
    )

    with open(
        path,
        "x",
    ) as log:
        subprocess.run(
            [
                "env",
                f"CXL_NUMA_NODE={numa_node}",
                dbtest(allocator),
                "--verbose",
                "--pin-cpus",
                "--runtime",
                str(TIME),
                "--num-threads",
                str(thread_count),
                "--scale-factor",
                str(scale_factor),
                "--bench",
                benchmark,
            ]
            + (
                [
                    "--bench-opts",
                    f"--workload-mix={','.join(map(str, WORKLOADS[workload]))}",
                ]
                if workload is not None
                else []
            ),
            stdout=log,
            stderr=log,
            check=False,
        )


def dbtest(allocator: str) -> str:
    return f"{ROOT}/../extern/silo/out-perf.masstree.{allocator}/benchmarks/dbtest"


if __name__ == "__main__":
    main()
