// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: MIT

#include "prime_finder.h"

#include <chrono>

namespace {
constexpr int chunk_size = 2000;

bool is_prime(int n)
{
    for (int d = 2; d * d <= n; ++d) {
        if (n % d == 0)
            return false;
    }
    return true;
}
}

std::unique_ptr<PrimeFinder> PrimeFinder::create()
{
    return std::make_unique<PrimeFinderImpl>();
}

void PrimeFinderImpl::start()
{
    next = 2;
    count = 0;
    limit = get_limit();
    set_found(0);
    set_progress(0);
    set_busy(true);
    timer.start(slint::TimerMode::Repeated, std::chrono::milliseconds(16), [this] { search_chunk(); });
}

void PrimeFinderImpl::stop()
{
    timer.stop();
    set_busy(false);
}

void PrimeFinderImpl::search_chunk()
{
    const int end = std::min(limit, next + chunk_size - 1);
    for (; next <= end; ++next) {
        if (is_prime(next))
            ++count;
    }
    set_found(count);
    set_progress(limit > 0 ? static_cast<float>(next - 1) / static_cast<float>(limit) : 1.f);
    if (next > limit) {
        timer.stop();
        set_busy(false);
        invoke_finished(count);
    }
}
