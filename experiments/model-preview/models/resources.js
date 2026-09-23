export function disposeScene(scene){
  const geometries=new Set(),materials=new Set(),textures=new Set();
  scene.traverse(object=>{
    if(object.geometry)geometries.add(object.geometry);
    for(const material of object.material?(Array.isArray(object.material)?object.material:[object.material]):[]){
      materials.add(material);
      for(const value of Object.values(material))if(value?.isTexture)textures.add(value);
    }
    object.shadow?.map?.dispose();object.shadow?.mapPass?.dispose();
  });
  for(const geometry of geometries)geometry.dispose();
  for(const material of materials)material.dispose();
  for(const texture of textures)texture.dispose();
  scene.clear();
}
