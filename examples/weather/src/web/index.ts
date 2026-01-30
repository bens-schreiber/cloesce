import { Weather, WeatherReport } from '@generated/client';

declare global {
    interface Window {
        listReports: () => Promise<void>;
        saveReport: () => Promise<void>;
        addWeatherEntry: () => Promise<void>;
        uploadPhoto: () => Promise<void>;
        downloadPhoto: () => Promise<void>;
    }
}

const getValue = (id: string) => (document.getElementById(id) as HTMLInputElement).value;

const showResult = (outputId: string, result: any) => {
    const output = document.getElementById(outputId)!;
    const status = result.ok ? 'success' : 'error';
    const icon = result.ok ? '✓' : '✗';
    const data = result.ok && result.data ? `\n${JSON.stringify(result.data, null, 2)}` : '';
    output.innerHTML = `<div class="${status}">${icon} ${result.ok ? 'Success' : 'Error'} (${result.status})</div>${result.message || data}`;
};

window.listReports = async () => {
    const result = await WeatherReport.LIST("withWeatherEntries");
    showResult('list-output', result);
};

window.saveReport = async () => {
    const title = getValue('save-title');
    const description = getValue('save-desc');

    if (!title) {
        document.getElementById('save-output')!.innerHTML = '<div class="error">Please enter a title</div>';
        return;
    }

    const result = await WeatherReport.SAVE({ title, description });
    showResult('save-output', result);
};

window.addWeatherEntry = async () => {
    const reportId = parseInt(getValue('entry-report-id'));

    if (!reportId) {
        document.getElementById('entry-output')!.innerHTML = '<div class="error">Please enter a report ID</div>';
        return;
    }

    const getResult = await WeatherReport.GET(reportId, "withWeatherEntries");
    if (!getResult.ok) {
        showResult('entry-output', getResult);
        return;
    }

    const report = getResult.data!;
    const newEntry = {
        weatherReportId: reportId,
        location: getValue('entry-location') || '',
        temperature: parseFloat(getValue('entry-temp')) || 0,
        condition: getValue('entry-condition') || '',
        dateTime: getValue('entry-datetime') ? new Date(getValue('entry-datetime')) : new Date()
    };

    const result = await WeatherReport.SAVE({
        id: report.id,
        title: report.title,
        description: report.description,
        weatherEntries: [...(report.weatherEntries || []), newEntry]
    }, "withWeatherEntries");

    showResult('entry-output', result);
};

window.uploadPhoto = async () => {
    const id = parseInt(getValue('upload-id'));
    const fileInput = document.getElementById('upload-file') as HTMLInputElement;
    const output = document.getElementById('upload-output')!;

    if (!id) {
        output.innerHTML = '<div class="error">Please enter a weather entry ID</div>';
        return;
    }

    if (!fileInput.files?.[0]) {
        output.innerHTML = '<div class="error">Please select a file</div>';
        return;
    }

    const buffer = await fileInput.files[0].arrayBuffer();
    const weather = new Weather();
    weather.id = id;

    const result = await weather.uploadPhoto(new Uint8Array(buffer), "withPhoto");
    showResult('upload-output', result);
};

window.downloadPhoto = async () => {
    const id = parseInt(getValue('download-id'));

    if (!id) {
        document.getElementById('download-output')!.innerHTML = '<div class="error">Please enter a weather entry ID</div>';
        return;
    }

    const weather = new Weather();
    weather.id = id;
    const result = await weather.downloadPhoto("withPhoto");

    if (result.ok && result.data) {
        const blob = await result.data.blob();
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `weather-photo-${id}.jpg`;
        a.click();
        URL.revokeObjectURL(url);
    }

    showResult('download-output', result);
};