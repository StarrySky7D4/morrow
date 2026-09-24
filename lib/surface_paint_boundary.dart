import 'package:flutter/widgets.dart';

/// A paint budget, not padding: keeps relief inside the gap between neighbours
/// without changing layout, focus, semantics or the child's identity.
class SurfacePaintBoundary extends StatelessWidget {
  const SurfacePaintBoundary({super.key, required this.child, this.outset = 6});

  final Widget child;
  final double outset;

  @override
  Widget build(BuildContext context) => ClipRect(
    clipper: _SurfacePaintClipper(outset),
    clipBehavior: Clip.hardEdge,
    child: child,
  );
}

class _SurfacePaintClipper extends CustomClipper<Rect> {
  const _SurfacePaintClipper(this.outset);
  final double outset;

  @override
  Rect getClip(Size size) => (Offset.zero & size).inflate(outset);

  @override
  bool shouldReclip(_SurfacePaintClipper oldClipper) =>
      outset != oldClipper.outset;
}
