# this is a simple script for processing transmission reports
# specifically, it is used to calculate and them plot generation intervals

# read in transmission report

transmission_report <- read.csv("output/transmission_report_triangle.csv")

infector_time <- transmission_report$time[
    match(x = transmission_report$infected_by,
          table = transmission_report$target_id)]

infector_time[is.na(infector_time)] <- 0

transmission_report <- cbind(transmission_report, infector_time)

generation_interval <- transmission_report$time - transmission_report$infector_time

transmission_report <- cbind(transmission_report, generation_interval)

# plot a histogram of all the simulated generation intervals

hist(transmission_report$generation_interval, freq = FALSE)

# we may want to stratify to look at infectors relatively
# early in the epidemic, before depletion of susceptibles
# becomes a factor

hist(transmission_report$generation_interval[
    transmission_report$infector_time < 
    quantile(transmission_report$infector_time, probs = 0.25)],
    freq = FALSE, xlim = c(0, 6),
    main = "Simulated generation intervals \n
    restricted to onward transmission from first 25% of infections",
    xlab = "time")

# draw on the infectiousness rate function, but converted to a PDF
# in practice, this means rescaling its "height" so the area
# under the curve sums to 1

segments(x0 = 0, y0 = 0, x1 = 3, y1 = 0, col = "red", lwd = 3)
segments(x0 = 3, y0 = 0, x1 = 4, y1 = 2/3, col = "red", lwd = 3)
segments(x0 = 4, y0 = 2/3, x1 = 6, y1 = 0, col = "red", lwd = 3)
