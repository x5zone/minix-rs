#include <minix.h>

struct clock_state {
    int running;
};

#define CLOCK_FREQ 60

void clock_init(void)
{
    int x = CLOCK_FREQ;
}
