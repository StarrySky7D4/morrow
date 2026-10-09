import { media } from '@kit.MediaKit';
import { fileIo as fs } from '@kit.CoreFileKit';
import { BusinessError } from '@kit.BasicServicesKit';
import { MusicPlayer, MusicPlaybackSource, MusicPlaybackDescriptor, MusicPlaybackFiles,
  musicPlaybackSourceValid, musicPlaybackSourceCopy, musicPlaybackSameSource } from '../model/MusicPlayback';

// The file adapter returns an exact registry-owned descriptor after verified
// music export. This class never opens a caller path or internal source_uri.
export class PlatformMusicPlayer implements MusicPlayer {
  private source: MusicPlaybackSource;
  private admitted: MusicPlaybackSource;
  private files: MusicPlaybackFiles;
  private descriptor: MusicPlaybackDescriptor | undefined = undefined;
  private player: media.AVPlayer | undefined = undefined;
  private closing: boolean = false;
  private loaded: boolean = false;
  private playerReleased: boolean = false;
  private loading: Promise<void> | undefined = undefined;
  private releaseFlight: Promise<void> | undefined = undefined;
  private generation: number = 0;
  private state: (value: string) => void = (_value: string): void => {};
  private position: (value: number) => void = (_value: number): void => {};
  private duration: (value: number) => void = (_value: number): void => {};
  private failure: (value: string) => void = (_value: string): void => {};
  constructor(source: MusicPlaybackSource, files: MusicPlaybackFiles) {
    this.source = source; this.admitted = musicPlaybackSourceCopy(source); this.files = files;
  }
  onState(callback: (value: string) => void): void { this.state = callback; }
  onPosition(callback: (value: number) => void): void { this.position = callback; }
  onDuration(callback: (value: number) => void): void { this.duration = callback; }
  onError(callback: (value: string) => void): void { this.failure = callback; }
  private owned(): boolean {
    return musicPlaybackSourceValid(this.source) &&
      musicPlaybackSameSource(this.source, this.admitted) && this.files.ownsSource(this.source);
  }
  private admittedDescriptor(): boolean {
    const descriptor = this.descriptor;
    return !!descriptor && Number.isSafeInteger(descriptor.fd) && descriptor.fd >= 0 &&
      descriptor.token === this.admitted.token && descriptor.byteLength === this.admitted.byteLength &&
      descriptor.sha256 === this.admitted.sha256 && this.files.ownsDescriptor(this.source, descriptor);
  }
  private active(generation: number): boolean {
    return !this.closing && generation === this.generation && this.owned() && this.admittedDescriptor();
  }
  load(): Promise<void> {
    if (this.loaded || this.closing || !this.owned()) { return Promise.reject(new Error('歌曲租约无效或会话已打开')); }
    this.loaded = true;
    const loading = this.loadSource(++this.generation); this.loading = loading; return loading;
  }
  private async loadSource(generation: number): Promise<void> {
    const descriptor = await this.files.open(this.source); this.descriptor = descriptor;
    if (!this.active(generation)) { throw new Error('歌曲文件描述符未通过原租约核对'); }
    const info = await fs.stat(descriptor.fd);
    if (!this.active(generation) || !info.isFile() || info.size !== this.admitted.byteLength) { throw new Error('歌曲文件长度或租约不一致'); }
    const player = await media.createAVPlayer(); this.player = player;
    if (!this.active(generation)) { throw new Error('歌曲会话来源已失效'); }
    player.on('stateChange', (state: string): void => {
      if (!this.active(generation)) { return; }
      if (state === 'prepared') { this.duration(player.duration); this.position(player.currentTime); }
      this.state(state);
    });
    player.on('timeUpdate', (value: number): void => { if (this.active(generation)) { this.position(value); } });
    player.on('durationUpdate', (value: number): void => { if (this.active(generation)) { this.duration(value); } });
    player.on('error', (_error: BusinessError): void => {
      if (this.active(generation)) { this.failure('歌曲无法播放，请检查文件或更换音频格式。'); }
    });
    player.fdSrc = { fd: descriptor.fd, offset: 0, length: this.admitted.byteLength };
  }
  private requirePlayer(): media.AVPlayer {
    if (!this.player || !this.active(this.generation) || this.playerReleased) { throw new Error('歌曲会话无法控制'); }
    return this.player;
  }
  async prepare(): Promise<void> { await this.requirePlayer().prepare(); }
  async play(): Promise<void> { await this.requirePlayer().play(); }
  async pause(): Promise<void> { await this.requirePlayer().pause(); }
  async seek(value: number): Promise<void> {
    if (!Number.isFinite(value) || value < 0) { throw new Error('歌曲位置无效'); }
    this.requirePlayer().seek(Math.round(value), media.SeekMode.SEEK_CLOSEST);
  }
  release(): Promise<void> {
    // A rejected AVPlayer release keeps the original FD. A rejected FD close
    // keeps that descriptor and retries it without releasing the player twice.
    if (this.releaseFlight) { return this.releaseFlight; }
    if (!this.closing) { this.closing = true; this.generation++; }
    const releasing = this.releaseResources(); this.releaseFlight = releasing;
    releasing.then((): void => { this.releaseFlight = undefined; }, (_error: Error): void => { this.releaseFlight = undefined; });
    return releasing;
  }
  private async releaseResources(): Promise<void> {
    if (this.loading) { await this.loading.catch((_error: Error): void => {}); }
    if (this.player && !this.playerReleased) { await this.player.release(); this.playerReleased = true; }
    if (this.descriptor) { await this.files.close(this.descriptor); this.descriptor = undefined; }
    this.player = undefined;
  }
}

export async function createMusicPlayer(source: MusicPlaybackSource, files: MusicPlaybackFiles): Promise<MusicPlayer> {
  return new PlatformMusicPlayer(source, files);
}
