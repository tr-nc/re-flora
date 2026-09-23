// Explicit, versioned data only. No model URLs, executable fields or storage hooks.
export function validatePreset(input,definitions){
  const fail=message=>{throw new Error(`预设无效：${message}`);};
  const object=(v,name)=>{if(!v||typeof v!=='object'||Array.isArray(v))fail(name);return v;};
  const number=(v,min,max,name)=>{if(!Number.isFinite(v)||v<min||v>max)fail(name);return v;};
  const integer=(v,min,max,name)=>{number(v,min,max,name);if(!Number.isInteger(v))fail(name);return v;};
  const boolean=(v,name)=>{if(typeof v!=='boolean')fail(name);return v;};
  const color=(v,name)=>{if(typeof v!=='string'||!/^#[0-9a-f]{6}$/i.test(v))fail(name);return v.toLowerCase();};
  object(input,'根对象');if(input.version!==1)fail('不支持的版本');
  const definition=definitions.find(model=>model.id===input.model);if(!definition)fail('未知模型');
  const values=object(input.modelSettings,'模型参数'),modelSettings={};
  for(const control of definition.controls){
    const v=values[control.key];
    modelSettings[control.key]=control.type==='color'?color(v,control.label):control.type==='checkbox'?boolean(v,control.label):number(v,control.min,control.max,control.label);
  }
  const processing=object(input.processing,'像素处理'),animation=object(input.animation,'动画'),view=object(input.view,'视角'),appearance=object(input.appearance,'观察底色');
  if(!['orthographic','perspective'].includes(view.projection))fail('投影');
  const vector=(v,name)=>{if(!Array.isArray(v)||v.length!==3)fail(name);return v.map(x=>number(x,-1000,1000,name));};
  const position=vector(view.position,'相机位置'),target=vector(view.target,'观察目标');
  if(Math.hypot(...position.map((v,i)=>v-target[i]))<.1)fail('相机与目标重合');
  if(!['compare','model','pixel'].includes(input.layout))fail('布局');
  return {
    version:1,model:definition.id,modelSettings,layout:input.layout,wireframe:boolean(input.wireframe,'线框'),
    // Legacy processing.repair is deliberately ignored: repair is mandatory.
    processing:{resolution:integer(processing.resolution,8,128,'像素数'),levels:integer(processing.levels,0,12,'亮度色阶')},
    animation:{clip:integer(animation.clip,0,1000,'动画片段'),time:number(animation.time,0,36000,'动画时刻'),fps:integer(animation.fps,2,60,'动画采样帧率'),speed:number(animation.speed,.25,2,'播放速度')},
    view:{projection:view.projection,position,target,zoom:number(view.zoom,.55,2.5,'缩放')},
    appearance:{background:color(appearance.background,'底色'),checker:boolean(appearance.checker,'棋盘')},
  };
}
