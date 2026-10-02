#include "my_application.h"

#include "launch_policy.h"

struct _MyApplication {
  GtkApplication parent_instance;
};

G_DEFINE_TYPE(MyApplication, my_application, GTK_TYPE_APPLICATION)

static void my_application_activate(GApplication* application) {
  GList* windows = gtk_application_get_windows(GTK_APPLICATION(application));
  if (windows != nullptr) {
    gtk_window_present(GTK_WINDOW(windows->data));
    return;
  }
  auto* window = GTK_WINDOW(
      gtk_application_window_new(GTK_APPLICATION(application)));
  gtk_window_set_title(window, "Morrow Linux GTK prerequisite probe");
  gtk_window_set_default_size(window, 720, 240);
  auto* label = gtk_label_new(LinuxFoundationBoundary());
  gtk_label_set_line_wrap(GTK_LABEL(label), TRUE);
  gtk_label_set_selectable(GTK_LABEL(label), TRUE);
  gtk_widget_set_margin_start(label, 24);
  gtk_widget_set_margin_end(label, 24);
  gtk_widget_set_margin_top(label, 24);
  gtk_widget_set_margin_bottom(label, 24);
  gtk_container_add(GTK_CONTAINER(window), label);
  gtk_widget_show_all(GTK_WIDGET(window));
}

static void my_application_class_init(MyApplicationClass* klass) {
  G_APPLICATION_CLASS(klass)->activate = my_application_activate;
}

static void my_application_init(MyApplication*) {}

MyApplication* my_application_new() {
  g_set_prgname(APPLICATION_ID);
  return MY_APPLICATION(g_object_new(my_application_get_type(),
                                    "application-id", APPLICATION_ID,
                                    "flags", G_APPLICATION_NON_UNIQUE,
                                    nullptr));
}
