import subprocess as sp

ALLOCATORS = [
    "cxlalloc",
    "boost",
    "cxl-shm",
    "lightning",
]
NODE = 1


def main():
    with open("result.ndjson", "x") as out:
        for allocator in ALLOCATORS:
            for thread_total in [1, 2, 4, 8, 16, 32, 40]:
                for process_count in [1, 2, 4]:
                    thread_count = thread_total // process_count

                    if thread_count == 0:
                        continue

                    print(f"Running {allocator} with {thread_total}t {process_count}p")

                    sp.run(
                        [
                            "env",
                            f"CXL_NUMA_NODE={NODE}",
                            "target/release/cxlalloc-bench",
                            "process",
                            "--allocator",
                            allocator,
                            "--name",
                            "thread-test",
                            "--node",
                            str(NODE),
                            "--size",
                            str(2**32),
                            "--process-count",
                            str(process_count),
                            "--thread-count",
                            str(thread_count),
                            "thread-test",
                            "--object-count",
                            str(32000),
                        ],
                        stdout=out,
                    )


if __name__ == "__main__":
    main()
