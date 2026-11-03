import local_lighting;

export __extern_cpp int main() {
    uint cases=0u;
    for(uint flags=0u;flags<8u;++flags) {
        for(uint count=0u;count<=80u;++count) {
            U_LocalLightInfo info={};
            info.abi_version=LOCAL_LIGHT_GPU_ABI_VERSION;
            info.capacity=64u;info.count=count;info.flags=flags;
            bool enabled=(flags&4u)==0u;
            if(localLightingEnabled(info)!=enabled) return 1;
            if(localLightCount(info)!=(enabled?min(count,64u):0u)) return 2;
            info.abi_version=0u;
            if(localLightCount(info)!=0u) return 3;
            cases++;
        }
    }
    printf("local lighting switch: %u ABI/count/flag cases passed\n",cases);
    return 0;
}
