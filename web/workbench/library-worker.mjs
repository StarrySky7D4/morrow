import {inspectDeviceLibrary} from './device-identity.mjs';
self.onmessage=async({data})=>{
  try {
    if(data?.kind!=='inspect' || data.name!=='main') throw Error('Invalid managed library selection');
    self.postMessage({kind:'state',state:await inspectDeviceLibrary(data.name)});
  } catch(error) {self.postMessage({kind:'error',message:String(error)});}
  finally {self.close();}
};
