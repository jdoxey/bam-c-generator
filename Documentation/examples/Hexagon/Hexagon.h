#ifndef BAM_GENERATED_HEXAGON_H
#define BAM_GENERATED_HEXAGON_H

typedef struct Hexagon_struct {
    double sideLength;
} Hexagon;

Hexagon *Hexagon_new(double sideLength);
void Hexagon_init(Hexagon *self, double sideLength);
void Hexagon_delete(Hexagon *self);

double Hexagon_area(Hexagon *self);

#endif  // BAM_GENERATED_HEXAGON_H
