import {leafDefinition} from './leaf.js';
import {butterflyDefinition} from './butterfly.js';
import {appleDefinition} from './apple.js';
import {flowerDefinitions} from './flowers.js';
import {animalDefinitions} from './animals.js';

// New assets register here; they never add a render loop, camera or pixel pass.
export const modelDefinitions=[leafDefinition,butterflyDefinition,appleDefinition,...flowerDefinitions,...animalDefinitions];
// Preserve the URL from the misidentified prototype, but expose only the
// corrected name/model in the catalog and canonical URL.
export const definitionFor=id=>modelDefinitions.find(model=>model.id===(id==='star-strawberry'?'gillenia':id));
