/// Opaque references are resolved only by the host that issued them.
class PastePart {
  const PastePart.literal(this.literal) : ticket = '', selection = '';
  const PastePart.ticket(this.ticket, {this.selection = 'outputMarkdown'})
    : literal = '';
  final String ticket, literal, selection;
}

class PasteInsertion {
  const PasteInsertion({
    required this.id,
    required this.field,
    required this.before,
    required this.startUtf16,
    required this.endUtf16,
    required this.parts,
    required this.after,
  });
  final String id, field, before, after;
  final int startUtf16, endUtf16;
  final List<PastePart> parts;
}

class EditorFields {
  const EditorFields({
    required this.title,
    required this.description,
    required this.hypothesis,
    required this.conclusion,
    required this.todos,
  });
  final String title, description, hypothesis, conclusion, todos;
}
