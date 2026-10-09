import { common } from '@kit.AbilityKit';
import { MusicFiles } from '../model/MusicFiles';
import { MusicLibraryView } from '../model/MusicLibrary';
import { MusicPlayer, MusicPlaybackSource } from '../model/MusicPlayback';
import { createMusicPlayer } from './PlatformMusicPlayer';
@Entry
@Component
struct Index {
  // Compilation-only method. No UI handler calls it, and this probe is not installed.
  private compileOnly(context: common.UIAbilityContext): Promise<MusicPlayer> {
    const library = new MusicLibraryView();
    const source = new MusicPlaybackSource(); source.libraryRevision = library.library_revision;
    return createMusicPlayer(source, new MusicFiles(context));
  }
  build() { Column() { Text('Music SDK compile probe') } }
}
