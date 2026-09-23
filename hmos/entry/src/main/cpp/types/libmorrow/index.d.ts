import { NodeContent } from '@kit.ArkUI';
export const request: (input: string) => Promise<string>;
export const renderPreview: (content: NodeContent, title: string, detail: string, dark: boolean) => void;
export const releasePreview: (content: NodeContent) => void;

