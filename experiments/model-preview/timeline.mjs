// One clock for both views. Static assets simply have no clips.
// Existing clips loop by default; actions may opt into exact, held end poses.
export function sampleTime(time,duration,fps,loop=true) {
  if(!(duration>0)) return 0;
  const phase=loop?((time%duration)+duration)%duration:Math.max(0,Math.min(time,duration));
  if(!loop&&phase===duration)return duration;
  const frame=Math.min(Math.floor((phase+1e-10)*fps),Math.ceil(duration*fps)-1);
  return frame/fps;
}

export function advanceTime(time,delta,duration,speed=1,loop=true) {
  if(!(duration>0)) return 0;
  const next=time+Math.max(0,Math.min(delta,.1))*speed;
  return loop?next%duration:Math.max(0,Math.min(next,duration));
}

export function stepTime(time,duration,fps,direction,loop=true){
  if(!(duration>0&&fps>0))return 0;
  const count=Math.ceil(duration*fps);
  const index=!loop&&time>=duration?count:Math.floor((sampleTime(time,duration,fps,loop)+1e-10)*fps);
  return loop?((index+direction+count)%count)/fps:Math.min(duration,Math.max(0,Math.min(count,index+direction))/fps);
}
