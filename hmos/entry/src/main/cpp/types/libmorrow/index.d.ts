import { NodeContent } from '@kit.ArkUI';
export const request: (input: string) => Promise<string>;
/** Duplicates caller FDs synchronously. Rust consumes only the duplicates. */
export const prepareFile: (sourceFd: number, destinationFd: number, maxBytes: number) => Promise<string>;
export const importFile: (input: string, sourceFd: number) => Promise<string>;
/** Write into an app-private verification file before exposing a picker URI. */
export const exportFile: (input: string, destinationFd: number) => Promise<string>;
export const renderPreview: (content: NodeContent, title: string, detail: string, dark: boolean) => void;
export const releasePreview: (content: NodeContent) => void;

