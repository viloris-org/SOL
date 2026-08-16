#include <SDL.h>

#include <stdio.h>
#include <string.h>

int main(void) {
    if (SDL_Init(SDL_INIT_VIDEO) != 0) {
        fprintf(stderr, "SDL 2 video initialization failed: %s\n", SDL_GetError());
        return 1;
    }

    const char *driver = SDL_GetCurrentVideoDriver();
    if (driver == NULL || strcmp(driver, "wayland") != 0) {
        fprintf(stderr, "SDL 2 did not select its Wayland backend\n");
        SDL_Quit();
        return 2;
    }

    SDL_Window *window = SDL_CreateWindow(
        "SOL SDL compatibility probe",
        SDL_WINDOWPOS_UNDEFINED,
        SDL_WINDOWPOS_UNDEFINED,
        320,
        180,
        SDL_WINDOW_SHOWN
    );
    if (window == NULL) {
        fprintf(stderr, "SDL 2 window creation failed: %s\n", SDL_GetError());
        SDL_Quit();
        return 3;
    }

    puts("compat:sdl2:wayland");
    SDL_PumpEvents();
    SDL_Delay(250);
    SDL_DestroyWindow(window);
    SDL_Quit();
    return 0;
}
