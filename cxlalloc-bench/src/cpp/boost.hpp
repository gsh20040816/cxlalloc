#include <boost/interprocess/indexes/null_index.hpp>
#include <boost/interprocess/managed_shared_memory.hpp>

using namespace boost::interprocess;

typedef basic_managed_shared_memory<char, rbtree_best_fit<mutex_family>,
                                    null_index>
    wrap_rbtree;

wrap_rbtree wrap_open(const char *name, size_t size);
void *wrap_allocate(wrap_rbtree *shm, size_t size);
void wrap_deallocate(wrap_rbtree *shm, void *pointer);
void *wrap_handle_to_address(wrap_rbtree *shm, wrap_rbtree::handle_t handle);
wrap_rbtree::handle_t wrap_address_to_handle(wrap_rbtree *shm, void *address);
