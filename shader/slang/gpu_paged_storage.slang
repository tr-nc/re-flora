// Generic record storage ABI, owned with re-flora-vkn::GpuPagedStorage.
// Pointers need no descriptor array or giant storage-buffer descriptor range.
// Caller must hold a frame residency lease and initialize records before reads.
module gpu_paged_storage;
public struct GpuStorageHandle {
    public uint64_t page_table;
    public uint64_t len;
    public uint stride;
    public uint page_shift;
};
// Preconditions: index < len, T fits the declared stride/alignment. The CPU
// allocator validates layout/arithmetic; model sampling validates logical keys.
uint64_t recordAddress(GpuStorageHandle storage,uint64_t index) {
    uint64_t* pages=(uint64_t*)storage.page_table;
    uint64_t within=index & ((uint64_t(1)<<storage.page_shift)-1);
    return pages[index>>storage.page_shift]+within*uint64_t(storage.stride);
}
public T gpuStorageLoad<T>(GpuStorageHandle storage,uint64_t index) {
    return *((T*)recordAddress(storage,index));
}
public void gpuStorageStore<T>(GpuStorageHandle storage,uint64_t index,T value) {
    *((T*)recordAddress(storage,index))=value;
}
