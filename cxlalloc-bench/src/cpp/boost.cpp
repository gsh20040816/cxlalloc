#include <boost/interprocess/indexes/null_index.hpp>
#include <boost/interprocess/managed_shared_memory.hpp>

using namespace boost::interprocess;

wrap_rbtree wrap_open(const char *name, size_t size) {
  return wrap_rbtree(open_or_create_t{}, name, size);
}

void *wrap_allocate(wrap_rbtree *shm, size_t size) {
  return shm->allocate(size);
}

void wrap_deallocate(wrap_rbtree *shm, void *pointer) {
  shm->deallocate(pointer);
}

void *wrap_handle_to_address(wrap_rbtree *shm, wrap_rbtree::handle_t handle) {
  return shm->get_address_from_handle(handle);
}

wrap_rbtree::handle_t wrap_address_to_handle(wrap_rbtree *shm, void *address) {
  return shm->get_handle_from_address(address);
}

void *wrap_set_root(wrap_rbtree *shm, void *pointer) {
  auto handle = wrap_address_to_handle(shm, pointer);
  return shm->construct<wrap_rbtree::handle_t>("root")(handle);
}

void *wrap_get_root(wrap_rbtree *shm) {
  auto handle = shm->find<wrap_rbtree::handle_t>("root").first;
  return wrap_handle_to_address(handle);
}
