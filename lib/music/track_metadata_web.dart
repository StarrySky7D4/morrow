import 'package:file_selector/file_selector.dart';
import 'embedded_tags.dart';
import 'track_metadata.dart';

Future<TrackMetadata> readTrackMetadata(XFile source) async =>
    readEmbeddedTags(await source.readAsBytes());
