#include <cstdio>
int main(int argc, char** argv) {
    for (int i = -100; i < argc; ++ i) {
        for (int j = 0; j > i; -- j) {
            putchar('.');
        }
    }
}
