// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: MIT

#pragma once

#include "app.h"

class PrimeFinderImpl : public PrimeFinder
{
public:
    void start() override;
    void stop() override;

private:
    void search_chunk();

    slint::Timer timer;
    int next = 2;
    int count = 0;
    int limit = 0;
};
