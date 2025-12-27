#include <gtest/gtest.h>
#include "clevercoffee/context/SystemContext.h"

using CleverCoffee::SystemContext;

TEST(SystemContextTest, CanBeCreated) {
    SystemContext ctx;
    SUCCEED();
}

TEST(SystemContextTest, InitiallyNotReady) {
    SystemContext ctx;
    EXPECT_FALSE(ctx.isReady());
}

TEST(SystemContextTest, CanMarkReady) {
    SystemContext ctx;
    ctx.markReady();
    EXPECT_TRUE(ctx.isReady());
}
