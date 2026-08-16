#include <gtk/gtk.h>

static gboolean quit_application(gpointer data) {
    g_application_quit(G_APPLICATION(data));
    return G_SOURCE_REMOVE;
}

static void activate(GtkApplication *application, gpointer data) {
    (void)data;
    GdkDisplay *display = gdk_display_get_default();
    if (display == NULL || !g_str_has_prefix(G_OBJECT_TYPE_NAME(display), "GdkWayland")) {
        g_printerr("GTK 4 did not select its Wayland backend\n");
        g_application_quit(G_APPLICATION(application));
        return;
    }

    GtkWidget *window = gtk_application_window_new(application);
    gtk_window_set_title(GTK_WINDOW(window), "SOL GTK compatibility probe");
    gtk_window_set_default_size(GTK_WINDOW(window), 320, 180);
    gtk_window_present(GTK_WINDOW(window));
    g_print("compat:gtk4:wayland\n");
    g_timeout_add(250, quit_application, application);
}

int main(int argc, char **argv) {
    GtkApplication *application = gtk_application_new(
        "org.sol.compatibility.gtk4",
        G_APPLICATION_DEFAULT_FLAGS
    );
    g_signal_connect(application, "activate", G_CALLBACK(activate), NULL);
    int status = g_application_run(G_APPLICATION(application), argc, argv);
    g_object_unref(application);
    return status;
}
