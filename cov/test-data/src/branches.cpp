#include <cstdio>
int main(int argc, char** argv) {
    if (argc == 1) {
        printf("ok!\n");
        if (**argv == '?') {
            printf("what?\n");
        }
    }
    return 0;
}