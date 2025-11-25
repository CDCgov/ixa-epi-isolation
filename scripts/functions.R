#' triangle_vl
#' viral load at a given time based on triangular VL function
#'
#' @param t times at which viral load should be estimated
#' @param dp peak value of viral load -- assumes that peak VL > LOD
#' @param tp time of peak vira load
#' @param wp proliferation time (time from logVL = 0 up to peak)
#' @param wr clearance time (time from peak back down to logVL = 0)
#'
#' @return triangular viral load at each time t
#' @export
#'
#' @examples triangle_vl()
triangle_vl <- function(t, dp, tp, wp, wr) {
  # t: time at which to evaluate the triangular viral load
  # dp: peak value of viral load
  # tp: time of peak viral load
  # wp: proliferation time (time from logVL = 0 up to peak)
  # wr: clearance time (time from peak back down to logVL = 0)
  # The triangular viral load function is defined as follows:
  # if t <= tp, then VL(t) = dp / wp * (t - (tp - wp))
  # if t > tp, then VL(t) = dp - (dp / wr) * (t - tp)
  # This function returns the viral load at time t.
  dplyr::if_else(t <= tp,
    (dp / wp) * (t - (tp - wp)),
    dp - (dp / wr) * (t - tp)
  )
}

calculate_weibull_scale <- function(
  si_beta_0_exponentiated, si_beta_wr, wr, wr_mean, wr_sd
) {
  # We need wr_raw for the discrete Weibull scale parameter.
  # wr_raw is the individual-level standard normal deviation of
  # an individual's clearance time from the pooled mean.
  # recall that wr = exp(log_wr_mean + log_wr_sd * wr_raw) * wr_midpoint_prior
  # We output wr_mean = exp(log_wr_mean) * wr_midpoint_prior and
  # wr_sd = exp(log_wr_sd) from the Stan model.
  # Substituting gives us
  # = wr_raw = (ln(wr / wr_midpoint_prior) - log_wr_mean) / log_wr_sd
  # = (ln(wr / wr_midpoint_prior) - ln(wr_mean / wr_midpoint_prior)) / log_wr_sd
  # = ln((wr / wr_midpoint_prior) / (wr_mean / wr_midpoint_prior)) / log_wr_sd
  # = ln((wr / wr_midpoint_prior) * (wr_midpoint_prior / wr_mean)) / log_wr_sd
  # = ln(wr / wr_mean) / log_wr_sd
  log_wr_sd <- log(wr_sd)
  wr_raw <- log(wr / wr_mean) / log_wr_sd
  # Recall that the discrete Weibull takes two arguments --
  # a shape parameter (pooled/fit by symptom category), and
  # a scale parameter (individual level).
  # The scale parameter is
  # = exp(si_beta_0 + si_beta_wr * wr_raw)
  # = exp(si_beta_0) * exp(si_beta_wr * wr_raw).
  # We want to return the scale parameter for drawing
  # samples from the corresponding Weibull
  si_beta_0_exponentiated * exp(si_beta_wr * wr_raw)
}
