import { cryptoFramework } from '@kit.CryptoArchitectureKit';
import { util } from '@kit.ArkTS';

export async function editorInputHash(request: string): Promise<string> {
  const digest = cryptoFramework.createMd('SHA256');
  await digest.update({ data: new util.TextEncoder().encodeInto(request) });
  const result = await digest.digest();
  let hex: string = '';
  for (const byte of result.data) { hex += byte.toString(16).padStart(2, '0'); }
  return hex;
}
