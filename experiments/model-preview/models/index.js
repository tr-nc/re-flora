import {leafDefinition} from './leaf.js';
import {butterflyDefinition} from './butterfly.js';
import {appleDefinition} from './apple.js';
import {flowerDefinitions} from './flowers.js';

// New assets register here; they never add a render loop, camera or pixel pass.
export const modelDefinitions=[leafDefinition,butterflyDefinition,appleDefinition,...flowerDefinitions];
export const definitionFor=id=>modelDefinitions.find(model=>model.id===id);
