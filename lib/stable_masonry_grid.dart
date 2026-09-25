import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_staggered_grid_view/flutter_staggered_grid_view.dart';

import 'render_stable_masonry_grid.dart';

/// Keeps keyed card state while invalidating positions when order changes.
class StableMasonryGrid extends SliverMultiBoxAdaptorWidget {
  const StableMasonryGrid({
    super.key,
    required super.delegate,
    required this.ids,
    required this.gridDelegate,
    this.mainAxisSpacing = 0,
    this.crossAxisSpacing = 0,
  });

  final List<String> ids;
  final SliverSimpleGridDelegate gridDelegate;
  final double mainAxisSpacing;
  final double crossAxisSpacing;

  @override
  RenderStableMasonryGrid createRenderObject(BuildContext context) =>
      _IdentityMasonryGrid(
        ids: ids,
        childManager: context as SliverMultiBoxAdaptorElement,
        gridDelegate: gridDelegate,
        mainAxisSpacing: mainAxisSpacing,
        crossAxisSpacing: crossAxisSpacing,
      );

  @override
  void updateRenderObject(
    BuildContext context,
    RenderStableMasonryGrid renderObject,
  ) {
    (renderObject as _IdentityMasonryGrid).updateIds(ids);
    renderObject
      ..gridDelegate = gridDelegate
      ..mainAxisSpacing = mainAxisSpacing
      ..crossAxisSpacing = crossAxisSpacing;
  }
}

class _IdentityMasonryGrid extends RenderStableMasonryGrid {
  _IdentityMasonryGrid({
    required List<String> ids,
    required super.childManager,
    required super.gridDelegate,
    required super.mainAxisSpacing,
    required super.crossAxisSpacing,
  }) : _ids = List.of(ids);

  List<String> _ids;

  void updateIds(List<String> ids) {
    if (listEquals(_ids, ids)) return;
    _ids = List.of(ids);
    resetPlacement();
  }
}
