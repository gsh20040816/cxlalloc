#pragma once

#include <boost/interprocess/indexes/null_index.hpp>
#include <boost/interprocess/managed_external_buffer.hpp>
#include <cstdint>
#include <memory>

using namespace boost::interprocess;

class ManagedExternalBuffer {
public:
  typedef basic_managed_external_buffer<
      char, rbtree_best_fit<mutex_family, offset_ptr<void>>, null_index>
      Backend;

  Backend inner;

  ManagedExternalBuffer(open_only_t open, char *buffer, size_t size)
      : inner(open, buffer, size) {}

  ManagedExternalBuffer(create_only_t create, char *buffer, size_t size)
      : inner(create, buffer, size) {}
};

std::shared_ptr<ManagedExternalBuffer> managed_open(char *buffer, size_t size);
std::shared_ptr<ManagedExternalBuffer> managed_create(char *buffer,
                                                      size_t size);

static_assert(sizeof(ManagedExternalBuffer::Backend::handle_t) == 8);

char *managed_allocate(ManagedExternalBuffer *buffer, size_t size);
void managed_deallocate(ManagedExternalBuffer *buffer, char *pointer);
char *managed_handle_to_address(ManagedExternalBuffer *buffer, uint64_t handle);
uint64_t managed_address_to_handle(ManagedExternalBuffer *buffer,
                                   char *address);
