#include <stdlib.h> // malloc, free

#include "Hexagon.h"

Hexagon *Hexagon_new(double sideLength) {
    Hexagon *self = malloc(sizeof(Hexagon));
    if (self == NULL) {
        return NULL;
    }
    Hexagon_init(self, sideLength);
    return self;
}

void Hexagon_init(Hexagon *self, double sideLength) {
    self->sideLength = sideLength;
}

void Hexagon_delete(Hexagon *self) {
    free(self);
}

double Hexagon_area(Hexagon *self) {
    double threeTimesSqrtOfThreeAllOverTwo = 2.59807621135;
    double area = threeTimesSqrtOfThreeAllOverTwo * self->sideLength * self->sideLength;
    return area;
}
