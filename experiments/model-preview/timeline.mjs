// One clock for both views. Static assets simply have no clips.
export function sampleTime(time,duration,fps) {
  if(!(duration>0)) return 0;
  const phase=((time%duration)+duration)%duration;
  const frame=Math.min(Math.floor((phase+1e-10)*fps),Math.ceil(duration*fps)-1);
  return frame/fps;
}

export function advanceTime(time,delta,duration,speed=1) {
  if(!(duration>0)) return 0;
  return (time+Math.max(0,Math.min(delta,.1))*speed)%duration;
}
