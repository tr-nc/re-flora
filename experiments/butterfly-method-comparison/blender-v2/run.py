#!/usr/bin/env python3
"""Review-only butterfly v2. Default rebuilds; --export-existing preserves edits.

/usr/bin/python3 run.py --draft  # first 16px pose review, no replay/turntable yet
/usr/bin/python3 run.py          # full saved-source replay and render package
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import zlib

from PIL import Image

ROOT=Path(__file__).resolve().parent
BLENDER=os.environ.get("BLENDER") or shutil.which("blender") or str(Path.home()/".local/opt/blender-4.5.13-linux-x64/blender")
ANGLES=[0,45,90,135,180]
BLUE=[(24,32,48),(63,93,124),(80,190,220),(205,246,248)]
GRAY=[(24,24,24),(80,80,80),(160,160,160),(240,240,240)]


def run_blender(mode,destination,existing=True):
    command=[BLENDER,"--background","--factory-startup"]
    if existing: command.append(str(ROOT/"butterfly-prototype.blend"))
    command += ["--python",str(ROOT/"create_export_blender.py"),"--","--mode",mode,"--output",str(destination)]
    print(f"Blender {mode}: {destination.name}",flush=True)
    with (ROOT/f"{mode}-{destination.name}.log").open("w") as log:
        subprocess.run(command,check=True,stdout=log,stderr=subprocess.STDOUT)


def indexed(image,palette):
    result=Image.new("P",image.size,0)
    result.putpalette([0,0,0]+[c for rgb in palette for c in rgb])
    indices=[]
    for r,g,b,a in image.convert("RGBA").getdata():
        index=0 if a<128 else 1+min(range(4),key=lambda i:sum((v-p)**2 for v,p in zip((r,g,b),palette[i])))
        indices.append(index)
    result.putdata(indices)
    result.info["transparency"]=0
    return result


def save_indexed(image,path):
    image.save(path,transparency=0,bits=4)
    data=path.read_bytes()
    out=[data[:8]]
    offset=8
    while offset<len(data):
        length=struct.unpack(">I",data[offset:offset+4])[0]
        kind=data[offset+4:offset+8]
        chunk=data[offset+8:offset+8+length]
        if kind==b"PLTE": chunk=chunk[:15]
        out.append(struct.pack(">I",len(chunk))+kind+chunk+struct.pack(">I",zlib.crc32(kind+chunk)))
        offset+=length+12
    path.write_bytes(b"".join(out))


def atlas(folder,size):
    result=Image.new("RGBA",(size*5,size*5))
    for row,angle in enumerate(ANGLES):
        for col in range(5):
            cell=Image.open(folder/f"view{angle:03d}_frame{col:02d}.png").convert("RGBA")
            assert cell.size==(size,size)
            result.paste(cell,(col*size,row*size))
    return result


def assemble(draft):
    source=json.loads((ROOT/"source-manifest.json").read_text())
    data=(ROOT/"butterfly-prototype.glb").read_bytes()
    length=struct.unpack("<I",data[12:16])[0]
    glb=json.loads(data[20:20+length])
    assert len(glb["animations"])==1
    animation=glb["animations"][0]
    targets=[{"name":glb["nodes"][c["target"]["node"]]["name"],"path":c["target"]["path"]} for c in animation["channels"]]
    assert set(t["name"] for t in targets)==set(source["required_animated_nodes"])
    assert {t["path"] for t in targets if t["name"]=="Flight pose"}=={"translation","rotation"}
    for sampler in animation["samplers"]:
        accessor=glb["accessors"][sampler["input"]]
        assert accessor["min"]==[0] and accessor["max"]==[1]
    full=atlas(ROOT/"raw",128)
    full.save(ROOT/"atlas-128-rgba.png")
    outputs={}
    for size in (16,32):
        primary=atlas(ROOT/"raw"/f"native{size}",size)
        primary.save(ROOT/f"atlas-{size}-rgba.png")
        for name,palette in (("blue",BLUE),("gray",GRAY)):
            image=primary if name=="blue" else Image.merge("RGBA",(*[primary.convert("L")]*3,primary.getchannel("A")))
            path=ROOT/f"atlas-{size}-{name}-indexed.png"
            save_indexed(indexed(image,palette),path)
            reopened=Image.open(path)
            rgba=reopened.convert("RGBA")
            outputs[path.name]={"dimensions":list(reopened.size),"mode":reopened.mode,
                "used_indices":sorted(set(reopened.getdata())),"palette_entries":len(reopened.getpalette())//3,
                "alpha_values":sorted(set(rgba.getchannel("A").getdata())),"sha256":hashlib.sha256(path.read_bytes()).hexdigest()}
            assert outputs[path.name]["used_indices"]==[0,1,2,3,4]
            assert outputs[path.name]["palette_entries"]==5 and outputs[path.name]["alpha_values"]==[0,255]
    for name in ("body-only","wing-only","no-abdomen-lag"):
        save_indexed(indexed(atlas(ROOT/"raw"/name,16),BLUE),ROOT/f"diagnostic-{name}-80.png")
    # Preserve a same-resampling comparison for judging the native-pixel decision.
    save_indexed(indexed(full.resize((80,80),Image.Resampling.BOX),BLUE),ROOT/"diagnostic-v1-box-method-80.png")
    primary=Image.open(ROOT/"atlas-16-blue-indexed.png").convert("RGBA")
    body=Image.open(ROOT/"diagnostic-body-only-80.png").convert("RGBA")
    no_lag=Image.open(ROOT/"diagnostic-no-abdomen-lag-80.png").convert("RGBA")
    wing_only=Image.open(ROOT/"diagnostic-wing-only-80.png").convert("RGBA")
    assert primary.tobytes()!=no_lag.tobytes(),"Abdomen-layer suppression did not change the final pixels"
    assert no_lag.tobytes()!=wing_only.tobytes(),"Body-layer suppression did not change the final pixels"
    metrics=[]
    for row,angle in enumerate(ANGLES):
        frames=[primary.crop((c*16,row*16,(c+1)*16,(row+1)*16)) for c in range(5)]
        metrics.append({"azimuth":angle,"unique_frames":len({f.tobytes() for f in frames}),
            "opaque_pixels":[sum(a==255 for a in f.getchannel("A").getdata()) for f in frames],
            "bboxes":[f.getchannel("A").getbbox() for f in frames],
            "body_only_unique_frames":len({body.crop((c*16,row*16,(c+1)*16,(row+1)*16)).tobytes() for c in range(5)}),
            "abdomen_layer_changed_pixels":[sum(a!=b for a,b in zip(frames[c].getdata(),no_lag.crop((c*16,row*16,(c+1)*16,(row+1)*16)).getdata())) for c in range(5)],
            "body_bob_pitch_layer_changed_pixels":[sum(a!=b for a,b in zip(no_lag.crop((c*16,row*16,(c+1)*16,(row+1)*16)).getdata(),wing_only.crop((c*16,row*16,(c+1)*16,(row+1)*16)).getdata())) for c in range(5)],
            "changed_pixels_to_next_including_wrap":[sum(a!=b for a,b in zip(frames[c].getdata(),frames[(c+1)%5].getdata())) for c in range(5)]})
    raw_endpoint=Image.open(ROOT/"raw/loop_endpoint_view045.png").convert("RGBA")
    raw_start=Image.open(ROOT/"raw/view045_frame00.png").convert("RGBA")
    native_endpoint=Image.open(ROOT/"raw/native16/loop_endpoint_view045.png").convert("RGBA")
    native_start=Image.open(ROOT/"raw/native16/view045_frame00.png").convert("RGBA")
    loop=raw_endpoint.tobytes()==raw_start.tobytes() and native_endpoint.tobytes()==native_start.tobytes()
    assert loop,"True source loop endpoint differs"
    boundary=[]
    for folder,size in (("",128),("native16",16),("native32",32)):
        for angle in ANGLES:
            for col in range(5):
                a=Image.open(ROOT/"raw"/folder/f"view{angle:03d}_frame{col:02d}.png").getchannel("A")
                boundary.append(all(max(a.crop(box).getdata())==0 for box in ((0,0,size,1),(0,size-1,size,size),(0,0,1,size),(size-1,0,size,size))))
    replay=[]
    if not draft:
        for folder in ("","native16","native32"):
            for angle in ANGLES:
                for col in range(5):
                    name=f"view{angle:03d}_frame{col:02d}.png"
                    replay.append(Image.open(ROOT/"raw"/folder/name).tobytes()==Image.open(ROOT/"replay"/folder/name).tobytes())
        assert all(replay),"Saved-source replay differs"
    validation={"draft":draft,"true_loop_endpoint_raw_and_native16_equal":loop,
        "all_raw_frames_clear_boundary":all(boundary),"boundary_frames_checked":len(boundary),"replay_frames_compared":len(replay),"replay_all_equal":all(replay) if replay else None,
        "glb_animation_count":1,"glb_channels":targets,"outputs":outputs,"pixel_pose_metrics":metrics,
        "body_reference_projections":"raw/body-reference-projections.json",
        "primary_pixel_method":source["primary_pixel_method"],
        "source_sha256":{name:hashlib.sha256((ROOT/name).read_bytes()).hexdigest() for name in ("butterfly-prototype.blend","butterfly-prototype.glb","create_export_blender.py","run.py")},
        "art_verdict":"See QUALITY_REVIEW.md for human-visible assessment and limits; dimensions, counts and channels alone do not approve art."}
    (ROOT/"validation.json").write_text(json.dumps(validation,indent=2)+"\n")
    assert all(boundary),"Raw pose clipped at canvas edge"
    print(json.dumps({"draft":draft,"loop_equal":loop,"replay_frames":len(replay),"metrics":metrics},indent=2),flush=True)


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--draft",action="store_true")
    parser.add_argument("--export-existing",action="store_true")
    args=parser.parse_args()
    if not args.export_existing: run_blender("create",ROOT,False)
    run_blender("export-glb",ROOT)
    run_blender("render",ROOT/"raw")
    if not args.draft:
        run_blender("render",ROOT/"replay")
        run_blender("preview",ROOT/"turntable-frames")
    assemble(args.draft)
    if not args.draft:
        subprocess.run(["ffmpeg","-hide_banner","-loglevel","error","-y","-framerate","15","-i",str(ROOT/"turntable-frames/turntable_%03d.png"),
            "-vf","format=yuv420p","-c:v","libvpx-vp9","-crf","24","-b:v","0",str(ROOT/"blender-source-turntable.webm")],check=True)
