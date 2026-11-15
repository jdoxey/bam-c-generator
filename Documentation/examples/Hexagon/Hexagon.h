# ifndef __HEXAGON_H__
# define __HEXAGON_H__

typedef struct Hexagon_struct {
    double sideLength;
} Hexagon;

Hexagon *Hexagon_new(double sideLength);
void Hexagon_init(Hexagon *self, double sideLength);
void Hexagon_delete(Hexagon *hexagon);

double Hexagon_area(Hexagon *hexagon);

#endif  // __HEXAGON_H__
