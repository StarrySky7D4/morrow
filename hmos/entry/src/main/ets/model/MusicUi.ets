import { MusicLibraryView } from './MusicLibrary';
import { MusicPlaybackView } from './MusicPlayback';

export class MusicRecoveryChoice {
  trackId: string = '';
  title: string = '';
  phase: string = '';
}
export class MusicSpoolView {
  spoolId: string = '';
  name: string = '';
  byteLength: string = '0';
  originalSaved: boolean = false;
  choices: MusicRecoveryChoice[] = [];
  canStart: boolean = false;
}
export class MusicUiView {
  library: MusicLibraryView = new MusicLibraryView();
  playback: MusicPlaybackView = new MusicPlaybackView();
  spools: MusicSpoolView[] = [];
  working: boolean = false;
  error: string = '';
  notice: string = '';
  controlsEnabled: boolean = false;
  canWrite: boolean = false;
  canInspectImport: boolean = false;
  canContinueImport: boolean = false;
  canReconcileImport: boolean = false;
  canFinishImport: boolean = false;
  canRetryBegin: boolean = false;
  canRetryRead: boolean = false;
  canRetryMutation: boolean = false;
  canDiscardKnownMutation: boolean = false;
  diagnostics: string[] = [];
  lyricsOpen: boolean = false;
  lyricsText: string = '';
  lyricsTitle: string = '';
  lyricsSource: string = '';
  lyricsUntimed: boolean = false;
  unsavedLyrics: string = '';
  unsavedLyricsTrackId: string = '';
  unsavedLyricsTitle: string = '';
  footerText: string = '';
}
export interface MusicUiCallbacks {
  action: (action: string, trackId: string, value: number, spoolId: string) => void;
  reorder: (trackIds: string[]) => void;
}
