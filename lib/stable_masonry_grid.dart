import 'package:flutter/widgets.dart';
import 'package:flutter_staggered_grid_view/flutter_staggered_grid_view.dart';

/// 0.7.0 retains a moved child's column after Flutter invalidates its offset.
/// Clear that placement hint before layout; keep the Element/State itself.
class StableMasonryGrid extends SliverMasonryGrid {
  const StableMasonryGrid({
    super.key,
    required super.delegate,
    required super.gridDelegate,
    super.mainAxisSpacing,
    super.crossAxisSpacing,
  });

  @override
  RenderSliverMasonryGrid createRenderObject(BuildContext context) =>
      _StableRenderMasonryGrid(
        childManager: context as SliverMultiBoxAdaptorElement,
        gridDelegate: gridDelegate,
        mainAxisSpacing: mainAxisSpacing,
        crossAxisSpacing: crossAxisSpacing,
      );
}

class _StableRenderMasonryGrid extends RenderSliverMasonryGrid {
  _StableRenderMasonryGrid({
    required super.childManager,
    required super.gridDelegate,
    required super.mainAxisSpacing,
    required super.crossAxisSpacing,
  });

  @override
  void performLayout() {
    RenderBox? child = firstChild;
    while (child != null) {
      final data = child.parentData! as SliverMasonryGridParentData;
      if (data.layoutOffset == null) {
        data.crossAxisIndex = null;
        if (data.index == 0) data.layoutOffset = 0;
      }
      child = childAfter(child);
    }
    super.performLayout();
  }
}
