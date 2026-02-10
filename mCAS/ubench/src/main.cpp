#include "csr.h"
#include "util.h"
#include <algorithm>
#include <asm/mtrr.h>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <fcntl.h>
#include <fstream>
#include <hdr/hdr_histogram.h>
#include <iostream>
#include <iterator>
#include <sstream>
#include <stdio.h>
#include <stdlib.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <sys/types.h>
#include <thread>
#include <tuple>
#include <unistd.h>
#include <vector>

#define PARALLEL_INC_ITER 1000000
#define MAX_ATTEMPT 100000

#define SLEEP_SEC 1
#define PAGE_SIZE 4096
#define CSR_TIMER 7
#define CSR_START_OP 8
#define CSR_RD_BUFF 13
#define CSR_WR_BUFF 14

#define MIG_GROUP_SIZE 16

#define PARALLEL_THREADS 4

#define TEST_SLOT_CNT 32

static int thread_cnt = PARALLEL_THREADS;
static int iterations = PARALLEL_INC_ITER;
static bool test_basic = false;

uint64_t rand_seed;
uint64_t *wr_local_buffer;
uint64_t *rd_local_buffer;
struct hdr_histogram *hist_all[32];
uint64_t cpu_cas_share_data;

inline int pin_self_to_core(int core_id) {
  cpu_set_t set;
  CPU_ZERO(&set);
  CPU_SET(core_id, &set);
  int rc = pthread_setaffinity_np(pthread_self(), sizeof(set), &set);
  if (rc != 0) {
    LOG_ERROR("fail to pin core %d, %d\n", core_id, rc);
    return -1;
  } else {

    LOG_INFO("succeed to pin core %d\n", core_id);
  }
  return 0;
}

static inline void set_pthread_attr(struct sched_param *sparam,
                                    pthread_attr_t *attr) {

  pthread_attr_setinheritsched(attr, PTHREAD_EXPLICIT_SCHED);
  pthread_attr_setschedpolicy(attr, SCHED_FIFO);
  sparam->sched_priority = 1;
  pthread_attr_setschedparam(attr, sparam);
  LOG_INFO("Done set_pthread_attr\n");
}

int mtrr_set(uint64_t mtrr_base, uint64_t mtrr_size, int mtrr_type) {
  int fd;
  struct mtrr_sentry sentry;

  sentry.base = mtrr_base;
  sentry.size = mtrr_size;
  sentry.type = mtrr_type;
  if ((fd = open("/proc/mtrr", O_RDWR, 0)) == -1) {
    if (errno == ENOENT) {
      fputs("/proc/mtrr not found: not supported or you don't have a PPro?\n",
            stderr);
      exit(3);
    }
    LOG_INFO("Error opening /proc/mtrr\n");
    return -1;
  }

  if (ioctl(fd, MTRRIOC_ADD_ENTRY, &sentry) == -1) {
    LOG_INFO("Error doing ioctl(2) on /dev/mtrr\n");
    return -1;
  }
  return 0;
}

uint32_t global_aux;
static inline uint64_t rdtscp(uint32_t &aux) {
  uint64_t rax, rdx;
  asm volatile("rdtscp\n" : "=a"(rax), "=d"(rdx), "=c"(aux) : :);
  return (rdx << 32) + rax;
}

uint64_t getPercentile(std::vector<uint64_t> &data, double percentile) {
  if (data.empty())
    return 0.0;

  size_t index = static_cast<size_t>(percentile / 100.0 * (data.size() - 1));
  std::nth_element(data.begin(), data.begin() + index, data.end());
  return data[index];
}

void analyze_stat(vector<uint64_t> &stat_arr) {
  printf("Percentile 50: %lu\n", getPercentile(stat_arr, 50));
  printf("Percentile 90: %lu\n", getPercentile(stat_arr, 90));
  printf("Percentile 99: %lu\n", getPercentile(stat_arr, 99));
}

double cycle_to_byte(uint64_t cycle_delta, uint64_t byte_copied,
                     double cycle_to_ns) {
  return (double)(byte_copied) / (double)(cycle_delta * cycle_to_ns) *
         pow(10, 9) / (double)(2 << 20);
}

int get_addr_from_kmod(const char *file_name, uint64_t byte_size,
                       uint64_t *ret_pa_val, uint64_t **ret_va_ptr,
                       uint64_t *pci_vaddr_ptr, int csr_num) {
  int kmod_fd;
  ssize_t bytes_read;

  if (file_name == NULL) {
    LOG_ERROR("BAD file name\n");
    return -1;
  }
  LOG_INFO("Opening %s ...\n", file_name);
  kmod_fd = open(file_name,
                 O_RDWR); // FIXED: we can't open the file in read-only mode if
                          // we want to map it for both read and write
  if (kmod_fd == -1) {
    LOG_ERROR(" Failed with open %s\n", file_name);
    goto FAILED;
  }
  if ((bytes_read = read(kmod_fd, ret_pa_val, sizeof(uint64_t))) < 0) {
    LOG_ERROR(" Read %s from proc file failed. Remember to use sudo. \n",
              file_name);
    goto FAILED;
  }
  LOG_INFO(" %s read from the proc is 0x%lx (size=%ld)\n", file_name,
           *ret_pa_val, bytes_read);
  if (csr_num > 0) {
    pci_vaddr_ptr[csr_num] = *ret_pa_val;
    LOG_INFO(" pci_vaddr_ptr[%d] = 0x%lx\n", csr_num, pci_vaddr_ptr[csr_num]);
  }
  *ret_va_ptr = (uint64_t *)mmap(NULL, byte_size, PROT_READ | PROT_WRITE,
                                 MAP_SHARED, kmod_fd, 0);

  if (*ret_va_ptr == (void *)-1) {
    LOG_ERROR(" HPPB buffer mmap not successful. Found -1. buf size: %ld\n",
              byte_size);
    goto FAILED;
  }
  if (*ret_va_ptr == (void *)0) {
    LOG_ERROR(" HPPB buffer mmap not successful. Found 0.\n");
    goto FAILED;
  }
  return 0;
FAILED:
  return -1;
}

int init(int *pci_fd, uint64_t **pci_vaddr, uint64_t **rd_buff_vaddr,
         uint64_t *rd_buff_paddr, uint64_t **wr_buff_vaddr,
         uint64_t *wr_buff_paddr, uint64_t **target_buff_vaddr,
         uint64_t *target_buff_paddr) {

  int init_ok;
  uint64_t *pci_vaddr_ptr;
  /* Initialize CSR access */
  init_ok = init_csr(pci_fd, &(*pci_vaddr));
  if (init_ok) {
    LOG_ERROR(" Failed with init csr.\n");
    return -1;
  }
  pci_vaddr_ptr = *pci_vaddr;

  uint64_t TGT_SIZE = 64 * 1024 * 1024;
  uint64_t RD_SIZE = 64 * 1024;
  uint64_t WR_SIZE = 64 * 1024;

  init_ok =
      get_addr_from_kmod("/proc/mcas_target_buff", TGT_SIZE, target_buff_paddr,
                         target_buff_vaddr, pci_vaddr_ptr, -1);
  if (init_ok) {
    LOG_ERROR(" Failed with init target page.\n");
    return -1;
  }
  LOG_INFO("*page_vaddr = 0x%lx\n", (uint64_t)*target_buff_vaddr);
  LOG_INFO("*page_paddr = 0x%lx\n", *target_buff_paddr);
  mtrr_set(*target_buff_paddr, TGT_SIZE, 0);
  LOG_INFO("*page_paddr mtrr end = 0x%lx\n",
           (uint64_t)(*target_buff_paddr) + TGT_SIZE);

  *rd_buff_paddr =
      (uint64_t)(((uint64_t)*target_buff_paddr) + TGT_SIZE - RD_SIZE);
  *rd_buff_vaddr =
      (uint64_t *)(((uint64_t)*target_buff_vaddr) + TGT_SIZE - RD_SIZE);
  pci_vaddr_ptr[CSR_RD_BUFF] = *rd_buff_paddr;
  LOG_INFO("*page_vaddr = 0x%lx\n", (uint64_t)*rd_buff_vaddr);
  LOG_INFO("*page_paddr = 0x%lx, reg: %d\n", pci_vaddr_ptr[CSR_RD_BUFF],
           CSR_RD_BUFF);

  *wr_buff_paddr =
      (uint64_t)(((uint64_t)*target_buff_paddr) + TGT_SIZE - RD_SIZE - WR_SIZE);
  *wr_buff_vaddr = (uint64_t *)(((uint64_t)*target_buff_vaddr) + TGT_SIZE -
                                RD_SIZE - WR_SIZE);
  pci_vaddr_ptr[CSR_WR_BUFF] = *wr_buff_paddr;
  LOG_INFO("*page_vaddr = 0x%lx\n", (uint64_t)*wr_buff_vaddr);
  LOG_INFO("*page_paddr = 0x%lx, reg: %d\n", pci_vaddr_ptr[CSR_WR_BUFF],
           CSR_WR_BUFF);

  return 0;
}

uint64_t test_atomic(uint64_t *pci_vaddr, uint64_t *op_buf_vaddr,
                     vector<uint64_t> &cpu_stat_arr,
                     vector<uint64_t> &fpga_stat_arr) {
  volatile uint64_t *op_buf_vaddr_vlt = op_buf_vaddr;
  uint64_t orig_val = op_buf_vaddr[0];
  const int test_iter = 1000000;
  uint64_t start_cycle_count, end_cycle_count, sum, fpga_sum, delta;
  sum = 0;
  fpga_sum = 0;

  LOG_INFO("test_atomic\n");
  LOG_INFO("Orignal val at 0: 0x%lx\n", orig_val);
  for (int i = 0; i < test_iter; i++) {
    op_buf_vaddr[0] = 1;
    // pci_vaddr[CSR_OP_WR_DATA] = 0xDEADBEEF00000001;

    // Trigger FPGA to start
    pci_vaddr[CSR_START_OP] = 1;

    // wait for the data to get swapped
    start_cycle_count = rdtscp(global_aux);
    while (op_buf_vaddr_vlt[0] == orig_val)
      ;
    end_cycle_count = rdtscp(global_aux);

    delta = (end_cycle_count - start_cycle_count);
    sum += delta;
    // cpu_stat_arr.push_back(delta);
    delta = pci_vaddr[CSR_TIMER];
    // fpga_stat_arr.push_back(delta);
    fpga_sum += delta;
  }
  LOG_INFO("avg sw cycle cnt over %d tests: %ld\n", test_iter, sum / test_iter);
  LOG_INFO("avg hw cycle cnt over %d tests: %ld\n", test_iter,
           fpga_sum / test_iter);

  // LOG_INFO("input val: 0x%lx, value at 0: 0x%lx\n",
  // pci_vaddr[CSR_OP_WR_DATA], op_buf_vaddr[0]); LOG_INFO("sw cycle cnt:
  // %ld\n", end_cycle_count - start_cycle_count);
  return 0;
}

#pragma GCC push_options
#pragma GCC optimize("-O0")
static inline void flush_block_fenced(uint64_t *addr, uint64_t size) {
  asm volatile("mov %[addr], %%r10\n"
               "xor %%r11, %%r11 \n"
               "mfence \n"
               "LOOP_CL_FLUSH_BLOCK%=: \n"
               "clwb (%%r11, %%r10) \n"
               "clflush (%%r11, %%r10) \n"
               "add $0x40, %%r11 \n"
               "cmp %[size], %%r11 \n"
               "jl LOOP_CL_FLUSH_BLOCK%= \n"
               :
               : [addr] "r"(addr), [size] "r"(size)
               : "r10", "r11");
}

static inline void flush_one(uint64_t *addr) {
  asm volatile("mov %[addr], %%r10\n"
               "clflush (%%r10) \n"
               "mfence \n"
               :
               : [addr] "r"(addr)
               : "r10");
}

static inline void movdir64b_addr(uint64_t *content_addr, uint64_t *wr_addr) {
  asm volatile("mov %[wr_addr], %%r10\n"
               "mov %[content_addr], %%r9\n"
               // "mfence \n"
               "movdir64b 0x0(%%r9), %%r10 \n"
               "sfence \n"
               :
               : [wr_addr] "r"(wr_addr), [content_addr] "r"(content_addr)
               : "r9", "r10");
}

static inline void movdir64b_addr_vlt(volatile uint64_t *content_addr,
                                      uint64_t *wr_addr) {
  asm volatile("mov %[wr_addr], %%r10\n"
               "mov %[content_addr], %%r9\n"
               "mfence \n"
               "movdir64b 0x0(%%r9), %%r10 \n"
               :
               : [wr_addr] "r"(wr_addr), [content_addr] "r"(content_addr)
               : "r9", "r10");
}

static inline void ntst_addr(uint64_t *content_addr, uint64_t *wr_addr) {
  asm volatile("mov %[wr_addr], %%r10\n"
               "mov %[content_addr], %%r9\n"
               "vmovdqa64 0x0(%%r9), %%zmm0 \n"
               "vmovntdq %%zmm0, 0x0(%%r10) \n"
               :
               : [wr_addr] "r"(wr_addr), [content_addr] "r"(content_addr)
               : "r9", "r10", "zmm0");
}

static inline void mcas_rd_nc(uint64_t *A, volatile uint64_t *B) {
  __asm__ __volatile__(
      ".intel_syntax noprefix\n\t"
      // "mfence \n"
      "movdqu xmm0, [rdi]\n\t" // Load 128 bits from A into xmm0
      // "mfence \n"
      "movdqu [rsi], xmm0\n\t" // Store 128 bits from xmm0 into B
      // "mfence \n"
      ".att_syntax prefix\n"
      :
      : "D"(A), "S"(B)
      : "xmm0");
}

void store_256bit_nt(uint64_t *dst, uint64_t val0, uint64_t val1, uint64_t val2,
                     uint64_t val3) {
  __asm__ __volatile__(
      ".intel_syntax noprefix\n\t"
      // Set lower 128 bits (val0, val1)
      "movq xmm0, rsi\n\t"        // val0 → xmm0[63:0]
      "movq xmm1, rdx\n\t"        // val1 → xmm1[63:0]
      "punpcklqdq xmm0, xmm1\n\t" // combine to xmm0: {val1, val0}

      // Set upper 128 bits (val2, val3)
      "movq xmm2, rcx\n\t"        // val2
      "movq xmm3, r8\n\t"         // val3
      "punpcklqdq xmm2, xmm3\n\t" // combine to xmm2: {val3, val2}

      // Insert both halves into ymm0
      "vinserti128 ymm0, ymm0, xmm2, 1\n\t" // ymm0 = {xmm2, xmm0}

      // Non-temporal store
      "vmovntdq [rdi], ymm0\n\t"
      ".att_syntax prefix\n"
      :
      : "D"(dst), "S"(val0), "d"(val1), "c"(val2), "r"(val3)
      : "xmm0", "xmm1", "xmm2", "xmm3", "ymm0", "memory");
  for (int i = 0; i < 4; i++) {
    LOG_INFO("dst %d: 0x%lx\n", i, dst[i]);
  }
}

uint64_t test_mcas_spin(uint64_t *rd_buff_vaddr, uint64_t *wr_buff_vaddr,
                        uint64_t *target_buff_vaddr, uint64_t cmp_val,
                        uint64_t swap_val, uint64_t target_pa,
                        vector<uint64_t> &cpu_stat_arr,
                        vector<uint64_t> &retry_stat_arr) {
  uint64_t ret;
  uint64_t succeed = 0;
  uint64_t iter = 0;
  uint64_t local_cmp_val = cmp_val;
  volatile uint64_t *rd_buff_vaddr_vlt = (volatile uint64_t *)rd_buff_vaddr;
  uint64_t t0, t1, t2, t3;

  t0 = rdtscp(global_aux);
  while (succeed == 0) {
    wr_local_buffer[0] = local_cmp_val;
    wr_local_buffer[1] = swap_val;
    wr_local_buffer[2] = target_pa;
    wr_local_buffer[3] = 0;
    movdir64b_addr(wr_local_buffer, wr_buff_vaddr);

    // ntst_addr(wr_local_buffer, wr_buff_vaddr);
    // LOG_INFO("wr_buff_vaddr val: 0x%lx\n", (uint64_t)wr_buff_vaddr);
    // store_256bit_nt(wr_buff_vaddr, local_cmp_val, swap_val, target_pa, 0);

    // t1 = rdtscp(global_aux);
    // ret = rd_buff_vaddr_vlt[0];
    // succeed = rd_buff_vaddr_vlt[1];
    // succeed = 1;
    // t2 = rdtscp(global_aux);

    mcas_rd_nc(rd_buff_vaddr, rd_local_buffer);
    // flush_one(rd_buff_vaddr);
    // movdir64b_addr(rd_buff_vaddr, rd_local_buffer);
    ret = rd_local_buffer[0];
    succeed = rd_local_buffer[1];

    // LOG_INFO("succeed: %lu, iter: %d, ret: %lu, cmp: %lu, target: %lu\n",
    //          succeed, iter, ret, local_cmp_val, swap_val);

    iter++;
    if (iter > MAX_ATTEMPT)
      break;
    local_cmp_val = ret;
  }
  t3 = rdtscp(global_aux);
  cpu_stat_arr.push_back(t3 - t0);
  retry_stat_arr.push_back(iter);
  LOG_INFO("target_pa val: 0x%lx, at va: 0x%lx\n", target_buff_vaddr[0],
           (uint64_t)target_buff_vaddr);
  LOG_INFO("succeed: %llu, iter: %d, final cmp: 0x%llx, ret: 0x%llx, target: "
           "0x%llx\n",
           succeed, iter, local_cmp_val, ret, swap_val);
  return 0;
}

void test_cpu_cas_spin_parallel(uint64_t *rd_buff_vaddr,
                                uint64_t *wr_buff_vaddr,
                                uint64_t *target_buff_vaddr, uint64_t target_pa,
                                int id, struct hdr_histogram *hist) {

  uint64_t ret, accu;
  volatile uint64_t *rd_buff_vaddr_vlt = rd_buff_vaddr;
  volatile uint64_t *target_buff_vaddr_vlt = target_buff_vaddr;
  uint64_t t0, t1, t2, t3;
  uint64_t iter_accu = 0;
  for (int i = 0; i < 1000; i++) {
    uint64_t succeed = 0;
    uint64_t iter = 0;
    uint64_t local_cmp_val = xorshf96(&rand_seed + id);
    uint64_t swap_val = xorshf96(&rand_seed + id);
    bool success = false;
    t0 = rdtscp(global_aux);
    while (!success) {
      // flush_one(target_buff_vaddr);
      success = __atomic_compare_exchange(
          target_buff_vaddr, // pointer to the atomic variable
          &local_cmp_val,    // expected value (will be updated if fails)
          &swap_val,         // desired value to store
          false,             // do not use weak cmpxchg
          __ATOMIC_SEQ_CST,  // success memory order
          __ATOMIC_SEQ_CST   // failure memory order
      );
      iter++;
      if (iter > 1000)
        break;
    }
    t3 = rdtscp(global_aux);

    hdr_record_value(hist, t3 - t0);
    iter_accu += iter;
  }
  LOG_INFO("iter avg = %f\n", (float)iter_accu / 1000.0);
}

void test_mcas_spin_parallel_inc(
    uint64_t *rd_buff_vaddr, uint64_t *wr_buff_vaddr,
    uint64_t *target_buff_vaddr, uint64_t target_pa, int id,
    struct hdr_histogram *hist,
    std::vector<std::tuple<uint64_t, uint64_t, uint64_t, uint64_t>>
        *result_arr_ptr) {

  struct sched_param sparam;
  pthread_attr_t attr;

  pin_self_to_core(id / 2);

  pthread_attr_init(&attr);
  set_pthread_attr(&sparam, &attr);

  uint64_t ret;
  volatile uint64_t *rd_buff_vaddr_vlt = rd_buff_vaddr;
  volatile uint64_t *target_buff_vaddr_vlt = target_buff_vaddr;
  uint64_t t0, t1, t2, t3;

  uint64_t *wr_thread_buff = &(wr_local_buffer[id * 8]);
  uint64_t *rd_thread_buff = &(rd_local_buffer[id * 8]);

  for (int i = 0; i < iterations; i++) {
    uint64_t succeed = 0;
    uint64_t iter = 0;
    uint64_t local_cmp_val = 0;
    uint64_t swap_val = 0;
    uint64_t offset = (i % TEST_SLOT_CNT) * 128;
    t0 = rdtscp(global_aux);
    while (succeed == 0) {
      wr_thread_buff[0] = local_cmp_val;
      wr_thread_buff[1] = local_cmp_val + 1;
      wr_thread_buff[2] = target_pa + offset;
      wr_thread_buff[3] = id;
      movdir64b_addr(wr_thread_buff, wr_buff_vaddr);

      // movdir64b_addr_vlt(rd_buff_vaddr_vlt, rd_thread_buff);
      mcas_rd_nc(rd_buff_vaddr, rd_thread_buff);
      ret = rd_thread_buff[0];
      succeed = rd_thread_buff[1];
      result_arr_ptr->push_back(
          std::tuple(succeed, ret, local_cmp_val, offset / 128));

      /*
uint64_t val = target_buff_vaddr_vlt[0];
if (i > 0 && val != ret) {
  LOG_ERROR("thread: %d, i: %d, zero target val: %ld, ret: %ld\n", id, i,
            val, ret);
}*/

      iter++;
      if (iter > 10000) {
        printf("BAD, thread: %d, failed, "
               "try to cmp: %d, swap: %d, found: %ld, iter: %d, ld val: %ld\n",
               id, wr_thread_buff[0], wr_thread_buff[1], ret, iter,
               target_buff_vaddr_vlt[0]);
        break;
      }
      local_cmp_val = ret;
    }
    t3 = rdtscp(global_aux);

    hdr_record_value(hist, t3 - t0);
  }
}

void test_mcas_spin_parallel_inc_opt(uint64_t *rd_buff_vaddr,
                                     uint64_t *wr_buff_vaddr,
                                     volatile uint64_t *target_buff_vaddr,
                                     uint64_t target_pa, int id,
                                     struct hdr_histogram *hist) {

  struct sched_param sparam;
  pthread_attr_t attr;

  pin_self_to_core(id / 2);

  pthread_attr_init(&attr);
  set_pthread_attr(&sparam, &attr);

  volatile uint64_t *rd_buff_vaddr_vlt = rd_buff_vaddr;

  uint64_t *wr_thread_buff = &(wr_local_buffer[id * 8]);
  uint64_t *rd_thread_buff = &(rd_local_buffer[id * 8]);

  uint64_t value = *target_buff_vaddr;

  wr_thread_buff[2] = target_pa;
  wr_thread_buff[3] = id;

  for (int i = 0; i < iterations; i++) {
    wr_thread_buff[0] = value;
    wr_thread_buff[1] = value + id + 1;

    uint64_t start = rdtscp(global_aux);

    movdir64b_addr(wr_thread_buff, wr_buff_vaddr);
    // movdir64b_addr_vlt(rd_buff_vaddr_vlt, rd_thread_buff);
    mcas_rd_nc(rd_buff_vaddr, rd_thread_buff);
    value = rd_thread_buff[0];
    uint64_t succeed = rd_thread_buff[1];

    if (!__builtin_expect(succeed, 1)) {
      while (succeed != 1) {
        wr_thread_buff[0] = value;
        wr_thread_buff[1] = value + id + 1;

        movdir64b_addr(wr_thread_buff, wr_buff_vaddr);
        // movdir64b_addr_vlt(rd_buff_vaddr_vlt, rd_thread_buff);
        mcas_rd_nc(rd_buff_vaddr, rd_thread_buff);
        value = rd_thread_buff[0];
        succeed = rd_thread_buff[1];
      }
    }

    uint64_t stop = rdtscp(global_aux);
    hdr_record_value(hist, stop - start);
    value++;
  }
}

void test_mcas_spin_parallel(uint64_t *rd_buff_vaddr, uint64_t *wr_buff_vaddr,
                             uint64_t *target_buff_vaddr, uint64_t target_pa,
                             int id, struct hdr_histogram *hist) {

  uint64_t ret, accu;
  volatile uint64_t *rd_buff_vaddr_vlt = rd_buff_vaddr;
  volatile uint64_t *target_buff_vaddr_vlt = target_buff_vaddr;
  uint64_t t0, t1, t2, t3;

  // for (int i = 0; i < 1000; i++) {
  uint64_t succeed = 0;
  uint64_t iter = 0;
  uint64_t local_cmp_val = xorshf96(&rand_seed);
  uint64_t swap_val = xorshf96(&rand_seed);
  t0 = rdtscp(global_aux);
  while (succeed == 0) {
    wr_local_buffer[0] = local_cmp_val;
    wr_local_buffer[1] = swap_val;
    wr_local_buffer[2] = target_pa;
    wr_local_buffer[3] = id;
    movdir64b_addr(wr_local_buffer, wr_buff_vaddr);

    mcas_rd_nc(rd_buff_vaddr, rd_local_buffer);
    ret = rd_local_buffer[0];
    succeed = rd_local_buffer[1];

    accu += target_buff_vaddr_vlt[0];

    iter++;
    if (iter > 1000)
      break;
    local_cmp_val = ret;
  }
  t3 = rdtscp(global_aux);

  hdr_record_value(hist, t3 - t0);
  // }
}

uint64_t test_mcas(uint64_t *rd_buff_vaddr, uint64_t *wr_buff_vaddr,
                   uint64_t *target_buff_vaddr, uint64_t cmp_val,
                   uint64_t swap_val, uint64_t target_pa) {
  uint64_t ret = 0;
  uint64_t succeed = 0;

  volatile uint64_t *rd_buff_vaddr_vlt = rd_buff_vaddr;
  volatile uint64_t *target_buff_vaddr_vlt = target_buff_vaddr;

  // special write
  wr_buff_vaddr[0] = cmp_val;
  wr_buff_vaddr[1] = swap_val;
  wr_buff_vaddr[2] = target_pa;
  wr_buff_vaddr[3] = 0;
  flush_block_fenced(wr_buff_vaddr, 64);

  /*
  for (int i = 0; i < 128; i++) {
          target_buff_vaddr[i] = i;
          rd_buff_vaddr[i] = i + 0xFFFF0000;
  }*/
  flush_block_fenced(target_buff_vaddr, 4096);
  /*
  for (int i = 0; i < 128; i++) {
          LOG_INFO("test_mcas t%d_buff: 0x%llx\n", i, target_buff_vaddr[i]);
          LOG_INFO("test_mcas t%d_buff: 0x%llx\n", i, rd_buff_vaddr[i]);
  }*/
  flush_block_fenced(rd_buff_vaddr, 64);
  ret = rd_buff_vaddr_vlt[0];
  succeed = rd_buff_vaddr_vlt[1];

  LOG_INFO("test_mcas rd_buff: 0x%lx\n", ret);
  LOG_INFO("test_mcas rd_buff: 0x%lx\n", succeed);
  // succeed = rd_buff_vaddr_vlt[1];

  LOG_INFO("test_mcas wr0: 0x%lx\n", wr_buff_vaddr[0]);
  LOG_INFO("test_mcas wr1: 0x%lx\n", wr_buff_vaddr[1]);
  LOG_INFO("test_mcas wr2: 0x%lx\n", wr_buff_vaddr[2]);
  // sleep(SLEEP_SEC);
  /*
  for (int i = 0; i < 8; i++) {
          LOG_INFO("test_mcas tr%d: 0x%llx\n", i, target_buff_vaddr_vlt[i]);
  }*/

  return ret;
}
#pragma GCC pop_options

uint64_t get_rand_offset(int max_offset, int batch_size) {
  // uint64_t offset = xorshf96(&rand_seed) % (max_offset - batch_size);
  uint64_t offset = xorshf96(&rand_seed) % (256);
  // rand_seed = offset;
  // uint64_t offset = 0;
  LOG_INFO("offset: %lu\n", offset);
  return offset;
}

void test_mcas_spin_parallel_inc_reader(uint64_t total,
                                        uint64_t *target_buff_vaddr) {
  pin_self_to_core(31);
  printf("Starting reader on core 31...\n");
  uint64_t current = 0;
  while (current < total) {
    uint64_t next = *((volatile uint64_t *)target_buff_vaddr);
    if (next < current) {
      printf("BAD, mCAS target decreased from %ld to %ld\n", current, next);
    } else {
      current = next;
    }
  }
}

void parallel_mcas_inc(uint64_t *rd_buff_vaddr, uint64_t *wr_buff_vaddr,
                       uint64_t *target_buff_vaddr, uint64_t target_pa,
                       int num_threads) {

  vector<thread> threads;
  std::vector<std::vector<uint64_t>> cpu_data(num_threads,
                                              std::vector<uint64_t>());
  std::vector<std::vector<uint64_t>> retry_data(num_threads,
                                                std::vector<uint64_t>());

  for (int i = 0; i < TEST_SLOT_CNT; i++) {
    target_buff_vaddr[i * 16] = 0;
    LOG_INFO("target pa: %d, val: %ld\n", i, target_buff_vaddr[i * 16]);
  }

  std::vector<std::tuple<uint64_t, uint64_t, uint64_t, uint64_t>>
      result_arr[num_threads];

  for (int id = 0; id < num_threads; id++) {
    int true_id = id * 2;
    struct hdr_histogram *hist;
    hdr_init(1, INT64_C(3600000000), 3, &hist);
    hist_all[id] = hist;
    threads.emplace_back(test_mcas_spin_parallel_inc,
                         &(rd_buff_vaddr[true_id * 8]),
                         &(wr_buff_vaddr[true_id * 8]), target_buff_vaddr,
                         target_pa, true_id, hist, &result_arr[id]);
  }

  int cnt = 0;
  for (auto &result : result_arr) {
    for (auto &i : result) {
      printf("thread: %d, %ld, %ld, %ld, pa %ld\n", cnt, std::get<0>(i),
             std::get<1>(i), std::get<2>(i), std::get<3>(i));
    }
    cnt++;
  }
  // threads.emplace_back(test_mcas_spin_parallel_inc_reader, num_threads *
  // iterations, target_buff_vaddr);
  for (auto &t : threads) {
    t.join();
  }
  for (int i = 0; i < TEST_SLOT_CNT; i++) {
    LOG_INFO("target pa: %d, val: %ld\n", i, target_buff_vaddr[i * 16]);
  }

  std::ostringstream oss;
  oss << "mcas_" << num_threads << ".csv";
  std::ofstream ofs(oss.str(), std::ios::out | std::ios::trunc);
  FILE *file = fopen(oss.str().c_str(), "w");

  for (int id = 1; id < num_threads; id++) {
    int64_t dropped = hdr_add(hist_all[0], hist_all[id]);
    if (dropped > 0) {
      printf("BAD, dropped %li histogram records from thread %d", dropped, id);
    }
  }

  hdr_percentiles_print(hist_all[0], file, 5, 1.0,
                        CSV); // Format CLASSIC/CSV supported.
  LOG_INFO("done inc test, value = %lu, expect: %lu\n", target_buff_vaddr[0],
           num_threads * iterations);
}

void parallel_mcas_test(uint64_t *rd_buff_vaddr, uint64_t *wr_buff_vaddr,
                        uint64_t *target_buff_vaddr, uint64_t target_pa,
                        int num_threads) {

  vector<thread> threads;
  std::vector<std::vector<uint64_t>> cpu_data(num_threads,
                                              std::vector<uint64_t>());
  std::vector<std::vector<uint64_t>> retry_data(num_threads,
                                                std::vector<uint64_t>());

  for (int id = 0; id < num_threads; id++) {
    int true_id = id * 2;
    struct hdr_histogram *hist;
    hdr_init(1, INT64_C(3600000000), 3, &hist);
    hist_all[id] = hist;
    threads.emplace_back(test_mcas_spin_parallel, &(rd_buff_vaddr[true_id * 8]),
                         wr_buff_vaddr, target_buff_vaddr, target_pa, true_id,
                         hist);
    /*
threads.emplace_back(test_cpu_cas_spin_parallel, &(rd_buff_vaddr[true_id * 8]),
wr_buff_vaddr, target_buff_vaddr, target_pa, true_id, hist);
    */
  }
  for (auto &t : threads) {
    t.join();
  }
  for (int id = 1; id < num_threads; id++) {
    uint64_t ret = hdr_add(hist_all[0], hist_all[id]);
    LOG_INFO("ret: %ld\n", ret);
  }
  std::ostringstream oss;
  oss << "mcas_" << num_threads << ".csv";
  std::ofstream ofs(oss.str(), std::ios::out | std::ios::trunc);
  FILE *file = fopen(oss.str().c_str(), "w");

  hdr_percentiles_print(hist_all[0], file, 5, 1.0,
                        CSV); // Format CLASSIC/CSV supported.
}

void arg_parse(int argc, char *argv[]) {
  int opt;
  // parse command-line options
  while ((opt = getopt(argc, argv, "t:i:b")) != -1) {
    switch (opt) {
    case 't':
      thread_cnt = atoi(optarg);
      break;
    case 'i':
      iterations = atoi(optarg);
      break;
    case 'b':
      test_basic = true;
      break;
    default:
      fprintf(stderr,
              "Usage: %s [-t threads] [-i iterations] [-b test basic]\n",
              argv[0]);
      exit(EXIT_FAILURE);
    }
  }
}

int main(int argc, char **argv) {
  uint64_t *pci_vaddr;
  uint64_t *rd_buff_vaddr;
  uint64_t rd_buff_paddr;
  uint64_t *wr_buff_vaddr;
  uint64_t wr_buff_paddr;
  uint64_t *target_buff_vaddr;
  uint64_t target_buff_paddr;

  int ret, pci_fd;
  uint64_t cycle;

  // init csr
  LOG_INFO("Initializing ...\n");
  ret =
      init(&pci_fd, &pci_vaddr, &rd_buff_vaddr, &rd_buff_paddr, &wr_buff_vaddr,
           &wr_buff_paddr, &target_buff_vaddr, &target_buff_paddr);
  IF_FAIL_THEN_EXIT

  arg_parse(argc, argv);

  if (posix_memalign((void **)&wr_local_buffer, 4096, 4096) != 0) {
    perror("posix_memalign failed");
    return 1;
  }
  if (posix_memalign((void **)&rd_local_buffer, 4096, 4096) != 0) {
    perror("posix_memalign failed");
    return 1;
  }

  // return 0;

  vector<uint64_t> retry_stat_arr;
  vector<uint64_t> cpu_stat_arr;
  vector<uint64_t> fpga_stat_arr;

  if (test_basic) {
    test_mcas_spin(rd_buff_vaddr, wr_buff_vaddr, target_buff_vaddr, 0, 6,
                   target_buff_paddr, cpu_stat_arr, retry_stat_arr);
    test_mcas_spin(rd_buff_vaddr, wr_buff_vaddr, target_buff_vaddr, 3, 9,
                   target_buff_paddr, cpu_stat_arr, retry_stat_arr);
    test_mcas_spin(rd_buff_vaddr, wr_buff_vaddr, target_buff_vaddr, 4, 1,
                   target_buff_paddr, cpu_stat_arr, retry_stat_arr);
  } else {
    target_buff_vaddr[0] = 0;
    flush_one(target_buff_vaddr);
    parallel_mcas_inc(rd_buff_vaddr, wr_buff_vaddr, target_buff_vaddr,
                      target_buff_paddr, thread_cnt);
  }

  /*
   */
  // for (int i = 0; i < 10; i++) {
  // 	test_mcas_spin(rd_buff_vaddr, wr_buff_vaddr, target_buff_vaddr,
  // 		i, i+1, target_buff_paddr, cpu_stat_arr, retry_stat_arr);
  // 		//xorshf96(&rand_seed), xorshf96(&rand_seed), target_buff_paddr,
  // cpu_stat_arr, retry_stat_arr);
  // }
  /*
      for (int i = 1; i < 16; i++) {
              printf("iter with thread cnt: %d
     ---------------------------------\n", i); parallel_mcas_test(rd_buff_vaddr,
     wr_buff_vaddr, target_buff_vaddr, target_buff_paddr, i);
              printf("----------------------------------------------------------\n");
      }*/

  /*
      ret += test_mcas(rd_buff_vaddr, wr_buff_vaddr, target_buff_vaddr, 0, 1,
  target_buff_paddr);
  printf("----------------------------------------------------------\n");
      ret += test_mcas(rd_buff_vaddr, wr_buff_vaddr, target_buff_vaddr, 1, 0,
  target_buff_paddr);
  printf("----------------------------------------------------------\n");
      ret += test_mcas(rd_buff_vaddr, wr_buff_vaddr, target_buff_vaddr,
  0xFFFF0000, 0x5, rd_buff_paddr);
  printf("----------------------------------------------------------\n");
      ret += test_mcas(rd_buff_vaddr, wr_buff_vaddr, target_buff_vaddr,
  0x3344EEEE, 0, target_buff_paddr);
  */

  LOG_INFO("Retry stats: \n");
  analyze_stat(retry_stat_arr);
  LOG_INFO("CPU stats: \n");
  analyze_stat(cpu_stat_arr);
  // analyze_stat(fpga_stat_arr);
  // print_2d_arr(stat_arr);

  // clean up
  free((void *)wr_local_buffer);
  free((void *)rd_local_buffer);
  clean_csr(pci_fd, pci_vaddr);
  LOG_INFO("Done.\n");
  return ret;
  // FAILED:
  LOG_ERROR(" Failure detected in main(), existing ... \n");
  return -1;
}
