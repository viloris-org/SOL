#include <QApplication>
#include <QGuiApplication>
#include <QTimer>
#include <QWidget>

#include <iostream>

int main(int argc, char **argv) {
    QApplication application(argc, argv);
    if (QGuiApplication::platformName() != QStringLiteral("wayland")) {
        std::cerr << "Qt 6 did not select its Wayland backend\n";
        return 2;
    }

    QWidget window;
    window.setWindowTitle(QStringLiteral("SOL Qt compatibility probe"));
    window.resize(320, 180);
    window.show();
    std::cout << "compat:qt6:wayland" << std::endl;
    QTimer::singleShot(250, &application, &QCoreApplication::quit);
    return application.exec();
}
