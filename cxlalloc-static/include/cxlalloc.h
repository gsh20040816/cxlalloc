#ifndef CXLALLOC_STATIC_H
#define CXLALLOC_STATIC_H

#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * Initialize the allocator for this process. This thread does not need to call
 * `cxlalloc_init_thread`.
 *
 * `heap_id` is an application-defined string used to correlate heaps between processes.
 * `heap_numa` is -1 or else a NUMA node to bind heap memory to.
 * `heap_backend` must be one of [mmap, shm, ivshmem].
 * `heap_size` is the initial heap size in bytes.
 * `thread_count` is the total number of threads that will call the allocator.
 * `thread_id` must be (1) unique for each thread and (2) less than `thread_count`.
 */
void cxlalloc_init_process(const char *heap_id,
	                           int8_t heap_numa,
	                           const char *heap_backend,
	                           size_t heap_size,
	                           uint16_t thread_count,
	                           uint16_t thread_id);

/**
 * Initialize the allocator at a fixed virtual address window. `fixed_base`
 * must be page-aligned and free in every process that attaches this heap.
 */
void cxlalloc_init_process_fixed(const char *heap_id,
	                               int8_t heap_numa,
	                               const char *heap_backend,
	                               size_t heap_size,
	                               uint16_t thread_count,
	                               uint16_t thread_id,
	                               uintptr_t fixed_base);

/**
 * Initialize the allocator at a fixed virtual address window with separate
 * small and large heap capacities.
 */
void cxlalloc_init_process_fixed_split(const char *heap_id,
	                                     int8_t heap_numa,
	                                     const char *heap_backend,
	                                     size_t small_heap_size,
	                                     size_t large_heap_size,
	                                     size_t large_reserve_size,
	                                     uint16_t thread_count,
	                                     uint16_t thread_id,
	                                     uintptr_t fixed_base);

/**
 * Unlink all shm objects used by this heap id. This is only supported for
 * heap_backend="shm"; missing objects are ignored.
 */
bool cxlalloc_unlink_heap(const char *heap_id, const char *heap_backend);

/**
 * Drop this process's current allocator mapping. Callers must ensure no other
 * thread is concurrently using the allocator.
 */
void cxlalloc_close_process(void);

/**
 * Release huge-allocation hazard records for a range of thread ids in the
 * current process mapping. Callers must ensure those thread ids are no longer
 * concurrently using the allocator.
 */
void cxlalloc_release_thread_range(uint16_t first, uint16_t count);

/**
 * Release huge-allocation hazard records for one thread id without changing the
 * process-owned thread id range used by cxlalloc_close_process().
 */
void cxlalloc_release_thread(uint16_t thread_id);

/** Enable or disable use of the large-allocation reserve on this thread. */
void cxlalloc_set_large_reserve_enabled(bool enabled);

/**
 * Initialize the allocator for this thread.
 *
 * `thread_id` must be (1) unique for each thread and (2) less than `thread_count`.
 */
void cxlalloc_init_thread(uint16_t thread_id);

void *cxlalloc_malloc(size_t size);

void cxlalloc_free(void *pointer);

void *cxlalloc_realloc(void *pointer, size_t size);

void *cxlalloc_memalign(size_t size, size_t alignment);

/**
 * Try to convert a pointer into a persistent offset. Returns false if the pointer was
 * not allocated in this heap.
 */
bool cxlalloc_pointer_to_offset(const void *pointer, uint64_t *offset);

/**
 * Convert a persistent offset into a pointer in this process address space.
 */
void *cxlalloc_offset_to_pointer(uint64_t offset);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* CXLALLOC_STATIC_H */
