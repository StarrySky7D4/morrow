#ifdef __cplusplus
#include "morrow_changes_metadata_v1.hpp"
#else
#include "morrow_changes_metadata_v1.h"
#endif
#include <stdio.h>
#include <string.h>
static int bookmark_contract(void) {
    uint8_t wire[MC_METADATA_V1_BOOKMARK_BYTES], encoded[MC_METADATA_V1_BOOKMARK_BYTES];
    mc_metadata_v1_bookmark value;
    memset(wire, 0x2a, sizeof(wire));
    wire[0] = 1;
    for (unsigned i = 0; i < 8; ++i) wire[33 + i] = (uint8_t)(i + 1);
    memset(wire + 41, 0xd7, 32);
#ifdef __cplusplus
    morrow::changes_metadata_v1::bookmark input;
    morrow::changes_metadata_v1::bookmark::wire_type output{};
    if (input.decode(wire, sizeof(wire)) || input.encode(output)) return 1;
    value = input.value;
    memcpy(encoded, output.data(), sizeof(encoded));
#else
    if (mc_metadata_v1_bookmark_decode(wire, sizeof(wire), &value, sizeof(value)) ||
        mc_metadata_v1_bookmark_encode(&value, encoded, sizeof(encoded))) return 1;
#endif
    if (value.sequence != UINT64_C(0x0807060504030201) ||
        memcmp(value.window_id, wire + 1, 32) ||
        memcmp(value.metadata_cursor, wire + 41, 32) ||
        memcmp(encoded, wire, sizeof(wire))) return 1;
    mc_metadata_v1_bookmark saved = value;
    wire[0] = 2;
    if (mc_metadata_v1_bookmark_decode(wire, sizeof(wire), &value, sizeof(value))
            != MC_METADATA_V1_CONTRACT || memcmp(&value, &saved, sizeof(value))) return 1;
    if (mc_metadata_v1_bookmark_decode(wire, sizeof(wire) - 1, &value, sizeof(value))
            != MC_METADATA_V1_INVALID || memcmp(&value, &saved, sizeof(value))) return 1;
    memset(encoded, 0xa5, sizeof(encoded));
    if (mc_metadata_v1_bookmark_encode(&value, encoded, sizeof(encoded) - 1)
            != MC_METADATA_V1_INVALID) return 1;
    value.sequence = 0;
    if (mc_metadata_v1_bookmark_encode(&value, encoded, sizeof(encoded))
            != MC_METADATA_V1_INVALID) return 1;
    for (unsigned i = 0; i < sizeof(encoded); ++i) if (encoded[i] != 0xa5) return 1;
    return 0;
}

static uint32_t u32(const unsigned char *p){return (uint32_t)p[0]|((uint32_t)p[1]<<8)|((uint32_t)p[2]<<16)|((uint32_t)p[3]<<24);}
int main(int argc,char **argv){
 if(argc!=2)return 2;
 if(bookmark_contract())return 10;
 FILE *file=fopen(argv[1],"rb");if(!file)return 3;
 unsigned char h[73],payload[2048];unsigned count=0,accepted=0;
 while(fread(h,1,1,file)==1){
  if(fread(h+1,1,72,file)!=72){fclose(file);return 4;}
  uint32_t n=u32(h+69);if(n>sizeof(payload)||fread(payload,1,n,file)!=n){fclose(file);return 5;}
  uint32_t result;
#ifdef __cplusplus
  morrow::changes_metadata_v1::metadata metadata;
  result=metadata.decode(payload,n,h+1,h+37,u32(h+33));
#else
  mc_metadata_v1 out;memset(&out,0xa5,sizeof(out));
  result=mc_metadata_v1_decode(payload,n,h+1,h+37,u32(h+33),&out,sizeof(out));
  if(result){unsigned char sentinel[sizeof(out)];memset(sentinel,0xa5,sizeof(sentinel));if(memcmp(&out,sentinel,sizeof(out)))return 9;}
#endif
  if((result==0)!=(h[0]!=0)){fprintf(stderr,"case %u mismatch code=%u\n",count,result);fclose(file);return 6;}
  accepted+=(result==0);count++;
 }
 if(ferror(file)||fclose(file))return 7;
 printf("cases=%u accepted=%u rejected=%u\n",count,accepted,count-accepted);return 0;
}
