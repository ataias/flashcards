// Shows how long the last HTML request took: the document request on a full
// page load, the HTMX request on an in-page update. Asset downloads and the
// DOM swap are deliberately outside the measurement.
(function () {
  var starts = new WeakMap();

  function format(milliseconds) {
    var rounded = Math.round(milliseconds);
    if (rounded < 1000) {
      return rounded + "ms";
    }
    return (rounded / 1000).toFixed(1) + "s";
  }

  function show(label, milliseconds) {
    var footer = document.getElementById("page-perf");
    if (footer) {
      footer.textContent = label + " " + format(milliseconds);
    }
  }

  function showNavigation() {
    var entry = performance.getEntriesByType("navigation")[0];
    if (entry && entry.requestStart > 0) {
      show("Page loaded in", entry.responseEnd - entry.requestStart);
    }
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", showNavigation);
  } else {
    showNavigation();
  }

  document.addEventListener("htmx:beforeSend", function (event) {
    starts.set(event.detail.xhr, performance.now());
  });

  document.addEventListener("htmx:afterRequest", function (event) {
    var start = starts.get(event.detail.xhr);
    starts.delete(event.detail.xhr);
    // Validation errors come back 200 + fragment and count as updates; 4xx/5xx
    // and network errors are not swapped, so they leave the last value alone.
    if (start !== undefined && event.detail.successful) {
      show("Updated in", performance.now() - start);
    }
  });
})();
