#include "boost.hpp"

using namespace boost::interprocess;

std::shared_ptr<ManagedExternalBuffer> managed_open(char *buffer, size_t size) {
  return std::shared_ptr<ManagedExternalBuffer>(
      new ManagedExternalBuffer(open_only_t{}, buffer, size));
}

std::shared_ptr<ManagedExternalBuffer> managed_create(char *buffer,
                                                      size_t size) {
  return std::shared_ptr<ManagedExternalBuffer>(
      new ManagedExternalBuffer(create_only_t{}, buffer, size));
}

char *managed_allocate(ManagedExternalBuffer *buffer, size_t size) {
  return (char *)buffer->inner.allocate(size);
}

void managed_deallocate(ManagedExternalBuffer *buffer, char *pointer) {
  return buffer->inner.deallocate((void *)pointer);
}

char *managed_handle_to_address(ManagedExternalBuffer *buffer,
                                uint64_t handle) {
  return (char *)buffer->inner.get_address_from_handle(
      static_cast<ManagedExternalBuffer::Backend::handle_t>(handle));
}

uint64_t managed_address_to_handle(ManagedExternalBuffer *buffer,
                                   char *address) {
  return static_cast<uint64_t>(
      buffer->inner.get_handle_from_address((void *)address));
}
