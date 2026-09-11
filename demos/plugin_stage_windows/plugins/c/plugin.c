#include "../stage_common.h"
/* Bounded decimal parser/formatter: no locale, system services or printf. */
static int number(mp_span s, double *out) {
  if (!s.length || s.length > 32)
    return 0;
  double v = 0, scale = 1;
  int sign = 1, digits = 0, dot = 0;
  uint32_t i = 0;
  if (s.data[0] == '-' || s.data[0] == '+') {
    sign = s.data[0] == '-' ? -1 : 1;
    i++;
  }
  for (; i < s.length; i++) {
    uint8_t c = s.data[i];
    if (c == '.' && !dot) {
      dot = 1;
      continue;
    }
    if (c < '0' || c > '9')
      return 0;
    digits++;
    if (dot) {
      scale *= 10;
      v += (c - '0') / scale;
    } else
      v = v * 10 + c - '0';
    if (v > 1000000)
      return 0;
  }
  *out = v * sign;
  return digits != 0;
}
static void fixed(double v, char *out) {
  int64_t n = (int64_t)(v * 1000 + (v < 0 ? -0.5 : 0.5));
  char digits[32];
  int i = 0, j = 0;
  if (n < 0) {
    out[j++] = '-';
    n = -n;
  }
  int64_t whole = n / 1000;
  do {
    digits[i++] = (char)('0' + whole % 10);
    whole /= 10;
  } while (whole);
  while (i)
    out[j++] = digits[--i];
  out[j++] = '.';
  out[j++] = (char)('0' + n / 100 % 10);
  out[j++] = (char)('0' + n / 10 % 10);
  out[j++] = (char)('0' + n % 10);
  out[j] = 0;
}
int32_t morrow_run(void) {
  static uint8_t input[MP_MAX_TASK_BYTES], output[MP_UI_MAX_BYTES],
      completion[MP_MAX_TASK_BYTES];
  stage_input s = {0};
  int32_t result = -1;
  uint32_t length = 0;
  if (!stage_open(&s, input, sizeof(input)))
    goto done;
  mp_span value = stage_text("20");
  int mode = 0, reverse = 0;
  if (s.update) {
    value = stage_find(&s, "title").text;
    mode = stage_find(&s, "mode").checked;
    reverse = stage_find(&s, "reverse").checked;
    if (stage_action(&s, "title", "edit", MP_UI_EDIT_TEXT))
      value = s.e.text;
    else if (stage_action(&s, "mode", "mode", MP_UI_SET_TOGGLE))
      mode = s.e.checked;
    else if (stage_action(&s, "reverse", "reverse", MP_UI_SET_TOGGLE))
      reverse = s.e.checked;
    else if (stage_action(&s, "example", "example", MP_UI_ACTIVATE))
      value = stage_text(mode ? "1" : "100");
    else if (stage_action(&s, "reset", "reset", MP_UI_ACTIVATE)) {
      value = stage_text("20");
      mode = reverse = 0;
    } else
      goto done;
  }
  double v = 0;
  int valid = number(value, &v);
  char converted[64];
  converted[0] = 0;
  const char *from = mode ? (reverse ? "英尺 ft" : "米 m")
                          : (reverse ? "华氏度 °F" : "摄氏度 °C");
  const char *to = mode ? (reverse ? "米" : "英尺") : (reverse ? "°C" : "°F");
  if (valid) {
    fixed(mode ? (reverse ? v * 0.3048 : v / 0.3048)
               : (reverse ? (v - 32) * 5 / 9 : v * 9 / 5 + 32),
          converted);
    strcat(converted, " ");
    strcat(converted, to);
  }
  mp_ui_node_v1 nodes[10];
  uint32_t n = 0;
  nodes[n++] = stage_node("root", "", MP_UI_COLUMN);
  nodes[n] = stage_node("title", "root", MP_UI_TEXT_INPUT);
  nodes[n].label = stage_text(from);
  nodes[n].text = value;
  nodes[n].action = stage_text("edit");
  nodes[n++].max_bytes = 32;
  nodes[n] = stage_node("mode", "root", MP_UI_TOGGLE);
  nodes[n].label = stage_text("长度模式（米与英尺）");
  nodes[n].action = stage_text("mode");
  nodes[n++].checked = mode;
  nodes[n] = stage_node("reverse", "root", MP_UI_TOGGLE);
  nodes[n].label = stage_text("反向换算");
  nodes[n].action = stage_text("reverse");
  nodes[n++].checked = reverse;
  nodes[n++] = stage_node("actions", "root", MP_UI_ROW);
  nodes[n] = stage_node("example", "actions", MP_UI_BUTTON);
  nodes[n].label = stage_text("填入示例");
  nodes[n++].action = stage_text("example");
  nodes[n] = stage_node("reset", "actions", MP_UI_BUTTON);
  nodes[n].label = stage_text("重置");
  nodes[n++].action = stage_text("reset");
  nodes[n] = stage_node("result", "root", MP_UI_TEXT);
  nodes[n++].text = stage_text(valid ? converted : "等待有效数值");
  nodes[n] = stage_node("detail", "root", MP_UI_TEXT);
  nodes[n++].text = stage_text(
      valid ? (mode ? (reverse ? "米 = 英尺 × 0.3048" : "英尺 = 米 ÷ 0.3048")
                    : (reverse ? "°C = (°F − 32) × 5 ÷ 9"
                               : "°F = °C × 9 ÷ 5 + 32"))
            : "请输入普通十进制数，绝对值不超过 1,000,000");
  nodes[n] = stage_node("caption", "root", MP_UI_TEXT);
  nodes[n++].text = stage_text(
      "温度与长度双向换算；显示三位小数。输入错误会保留会话，可继续修改。");
  if (mp_ui_document_encode(1, sizeof(mp_ui_node_v1), nodes, n, output,
                            sizeof(output), &length) != MP_CODEC_OK)
    goto done;
  if (mp_task_output(s.task, output, length, completion, sizeof(completion),
                     &length) == MP_CODEC_OK)
    result = mp_wasm_task_complete(completion, length);
done:
  stage_close(&s);
  return result;
}
